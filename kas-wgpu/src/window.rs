// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! `Window` and `WindowList` types

use std::time::{Duration, Instant};

use kas::event::Callback;
use kas::geom::{AxisInfo, Margins, Size, SizeRules};
use kas::{event, TkAction, Widget};
use winit::dpi::LogicalSize;
use winit::error::OsError;
use winit::event::WindowEvent;
use winit::event_loop::EventLoopWindowTarget;

use crate::draw::DrawPipe;
use crate::theme::Theme;

/// Per-window data
pub struct Window<T> {
    widget: Box<dyn kas::Window>,
    /// The winit window
    pub(crate) window: winit::window::Window,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    timeouts: Vec<(usize, Instant, Option<Duration>)>,
    tk_window: TkWindow<T>,
}

// Public functions, for use by the toolkit
impl<T: Theme<DrawPipe>> Window<T> {
    /// Construct a window
    pub fn new<U: 'static>(
        adapter: &wgpu::Adapter,
        event_loop: &EventLoopWindowTarget<U>,
        mut widget: Box<dyn kas::Window>,
        theme: T,
    ) -> Result<Self, OsError> {
        let window = winit::window::Window::new(event_loop)?;
        let dpi_factor = window.hidpi_factor();
        let size: Size = window.inner_size().to_physical(dpi_factor).into();

        let (mut device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            extensions: wgpu::Extensions {
                anisotropic_filtering: false,
            },
            limits: wgpu::Limits::default(),
        });

        let surface = wgpu::Surface::create(&window);

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.0,
            height: size.1,
            present_mode: wgpu::PresentMode::Vsync,
        };
        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        let mut tk_window = TkWindow::new(&mut device, sc_desc.format, size, dpi_factor, theme);
        tk_window.ev_mgr.configure(widget.as_widget_mut());

        widget.resize(&mut tk_window, size);

        let w = Window {
            widget,
            window,
            device,
            queue,
            surface,
            sc_desc,
            swap_chain,
            timeouts: vec![],
            tk_window,
        };

        Ok(w)
    }

    /// Called by the `Toolkit` when the event loop starts to initialise
    /// windows. Optionally returns a callback time.
    pub fn init(&mut self) -> Option<Instant> {
        self.window.request_redraw();

        for (i, condition) in self.widget.callbacks() {
            match condition {
                Callback::Start => {
                    self.widget.trigger_callback(i, &mut self.tk_window);
                }
                Callback::Repeat(dur) => {
                    self.widget.trigger_callback(i, &mut self.tk_window);
                    self.timeouts.push((i, Instant::now() + dur, Some(dur)));
                }
            }
        }

        self.next_resume()
    }

    /// Recompute layout of widgets and redraw
    pub fn reconfigure(&mut self) {
        let size = Size(self.sc_desc.width, self.sc_desc.height);
        self.widget.resize(&mut self.tk_window, size);
        self.window.request_redraw();
    }

    /// Handle an event
    ///
    /// Return true to remove the window
    pub fn handle_event(&mut self, event: WindowEvent) -> TkAction {
        // Note: resize must be handled here to update self.swap_chain.
        match event {
            WindowEvent::Resized(size) => self.do_resize(size),
            WindowEvent::RedrawRequested => self.do_draw(),
            WindowEvent::HiDpiFactorChanged(factor) => {
                self.tk_window.set_dpi_factor(factor);
                self.do_resize(self.window.inner_size());
            }
            event @ _ => {
                event::Manager::handle_winit(&mut *self.widget, &mut self.tk_window, event)
            }
        }
        self.tk_window.pop_action()
    }

    pub(crate) fn timer_resume(&mut self, instant: Instant) -> (TkAction, Option<Instant>) {
        // Iterate over loop, mutating some elements, removing others.
        let mut i = 0;
        while i < self.timeouts.len() {
            for timeout in &mut self.timeouts[i..] {
                if timeout.1 == instant {
                    self.widget.trigger_callback(timeout.0, &mut self.tk_window);
                    if let Some(dur) = timeout.2 {
                        while timeout.1 <= Instant::now() {
                            timeout.1 += dur;
                        }
                    } else {
                        break; // remove
                    }
                }
                i += 1;
            }
            if i < self.timeouts.len() {
                self.timeouts.remove(i);
            }
        }

        (self.tk_window.pop_action(), self.next_resume())
    }

    fn next_resume(&self) -> Option<Instant> {
        let mut next = None;
        for timeout in &self.timeouts {
            next = match next {
                None => Some(timeout.1),
                Some(t) => Some(t.min(timeout.1)),
            }
        }
        next
    }
}

// Internal functions
impl<T: Theme<DrawPipe>> Window<T> {
    fn do_resize(&mut self, size: LogicalSize) {
        let size = size.to_physical(self.window.hidpi_factor()).into();
        if size == Size(self.sc_desc.width, self.sc_desc.height) {
            return;
        }
        self.widget.resize(&mut self.tk_window, size);

        let buf = self.tk_window.resize(&self.device, size);
        self.queue.submit(&[buf]);

        self.sc_desc.width = size.0;
        self.sc_desc.height = size.1;
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
    }

    fn do_draw(&mut self) {
        let frame = self.swap_chain.get_next_texture();
        self.tk_window.draw_iter(self.widget.as_widget());
        let buf = self.tk_window.render(&mut self.device, &frame.view);
        self.queue.submit(&[buf]);
    }
}

/// Implementation of [`kas::TkWindow`]
pub(crate) struct TkWindow<T> {
    draw_pipe: DrawPipe,
    action: TkAction,
    pub(crate) ev_mgr: event::Manager,
    theme: T,
}

impl<T: Theme<DrawPipe>> TkWindow<T> {
    pub fn new(
        device: &mut wgpu::Device,
        tex_format: wgpu::TextureFormat,
        size: Size,
        dpi_factor: f64,
        mut theme: T,
    ) -> Self {
        let draw_pipe = DrawPipe::new(device, tex_format, theme.get_fonts(), size);
        theme.set_dpi_factor(dpi_factor as f32);

        TkWindow {
            draw_pipe,
            action: TkAction::None,
            ev_mgr: event::Manager::new(dpi_factor),
            theme,
        }
    }

    pub fn set_dpi_factor(&mut self, dpi_factor: f64) {
        self.ev_mgr.set_dpi_factor(dpi_factor);
        self.theme.set_dpi_factor(dpi_factor as f32);
        // Note: we rely on caller to resize widget
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: Size) -> wgpu::CommandBuffer {
        self.draw_pipe.resize(device, size)
    }

    #[inline]
    pub fn pop_action(&mut self) -> TkAction {
        let action = self.action;
        self.action = TkAction::None;
        action
    }

    /// Iterate over a widget tree, queuing drawables
    pub fn draw_iter(&mut self, widget: &dyn kas::Widget) {
        self.theme.draw(&mut self.draw_pipe, &self.ev_mgr, widget);

        for n in 0..widget.len() {
            self.draw_iter(widget.get(n).unwrap());
        }
    }

    /// Render all queued drawables
    pub fn render(
        &mut self,
        device: &mut wgpu::Device,
        frame_view: &wgpu::TextureView,
    ) -> wgpu::CommandBuffer {
        let clear_color = self.theme.clear_colour().into();
        self.draw_pipe.render(device, frame_view, clear_color)
    }
}

impl<T: Theme<DrawPipe>> kas::TkWindow for TkWindow<T> {
    fn data(&self) -> &event::Manager {
        &self.ev_mgr
    }

    fn update_data(&mut self, f: &mut dyn FnMut(&mut event::Manager) -> bool) {
        if f(&mut self.ev_mgr) {
            self.send_action(TkAction::Redraw);
        }
    }

    fn size_rules(&mut self, widget: &dyn Widget, axis: AxisInfo) -> SizeRules {
        self.theme.size_rules(&mut self.draw_pipe, widget, axis)
    }

    fn margins(&self, widget: &dyn Widget) -> Margins {
        self.theme.margins(widget)
    }

    #[inline]
    fn redraw(&mut self, _: &dyn Widget) {
        self.send_action(TkAction::Redraw);
    }

    #[inline]
    fn send_action(&mut self, action: TkAction) {
        self.action = self.action.max(action);
    }
}