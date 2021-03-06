// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Event manager

// Without winit, several things go unused
#![cfg_attr(not(feature = "winit"), allow(unused))]

use linear_map::{set::LinearSet, LinearMap};
use log::trace;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;
use std::u16;

use super::*;
use crate::conv::Cast;
use crate::geom::Coord;
#[allow(unused)]
use crate::WidgetConfig; // for doc-links
use crate::{ShellWindow, TkAction, Widget, WidgetId, WindowId};

mod mgr_pub;
mod mgr_shell;

/// Controls the types of events delivered by [`Manager::request_grab`]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GrabMode {
    /// Deliver [`Event::PressMove`] and [`Event::PressEnd`] for each press
    Grab,
    /// Deliver [`Event::Pan`] events, with scaling and rotation
    PanFull,
    /// Deliver [`Event::Pan`] events, with scaling
    PanScale,
    /// Deliver [`Event::Pan`] events, with rotation
    PanRotate,
    /// Deliver [`Event::Pan`] events, without scaling or rotation
    PanOnly,
}

#[derive(Clone, Debug)]
struct MouseGrab {
    button: MouseButton,
    repetitions: u32,
    start_id: WidgetId,
    depress: Option<WidgetId>,
    mode: GrabMode,
    pan_grab: (u16, u16),
}

#[derive(Clone, Debug)]
struct TouchGrab {
    start_id: WidgetId,
    depress: Option<WidgetId>,
    cur_id: Option<WidgetId>,
    coord: Coord,
    mode: GrabMode,
    pan_grab: (u16, u16),
}

const MAX_PAN_GRABS: usize = 2;

#[derive(Clone, Debug)]
struct PanGrab {
    id: WidgetId,
    mode: GrabMode,
    source_is_touch: bool,
    n: u16,
    coords: [(Coord, Coord); MAX_PAN_GRABS],
}

#[derive(Clone, Debug)]
enum Pending {
    LostCharFocus(WidgetId),
    LostSelFocus(WidgetId),
}

/// Event manager state
///
/// This struct encapsulates window-specific event-handling state and handling.
/// Most operations are only available via a [`Manager`] handle, though some
/// are available on this struct.
///
/// Besides event handling, this struct also configures widgets.
///
/// Some methods are intended only for usage by KAS shells and are hidden from
/// documentation unless the `internal_doc` feature is enabled. Only [winit]
/// events are currently supported; changes will be required to generalise this.
///
/// [winit]: https://github.com/rust-windowing/winit
//
// Note that the most frequent usage of fields is to check highlighting states
// for each widget during drawing. Most fields contain only a few values, hence
// `SmallVec` is used to keep contents in local memory.
#[derive(Debug)]
pub struct ManagerState {
    config: Rc<RefCell<Config>>,
    end_id: WidgetId,
    modifiers: ModifiersState,
    /// char focus is on same widget as sel_focus; otherwise its value is ignored
    char_focus: bool,
    sel_focus: Option<WidgetId>,
    nav_focus: Option<WidgetId>,
    nav_fallback: Option<WidgetId>,
    nav_stack: SmallVec<[u32; 16]>,
    hover: Option<WidgetId>,
    hover_icon: CursorIcon,
    key_depress: LinearMap<u32, WidgetId>,
    last_mouse_coord: Coord,
    last_click_button: MouseButton,
    last_click_repetitions: u32,
    last_click_timeout: Instant,
    mouse_grab: Option<MouseGrab>,
    touch_grab: LinearMap<u64, TouchGrab>,
    pan_grab: SmallVec<[PanGrab; 4]>,
    accel_stack: Vec<(bool, HashMap<VirtualKeyCode, WidgetId>)>,
    accel_layers: HashMap<WidgetId, (bool, HashMap<VirtualKeyCode, WidgetId>)>,
    popups: SmallVec<[(WindowId, kas::Popup); 16]>,
    new_popups: SmallVec<[WidgetId; 16]>,
    popup_removed: SmallVec<[(WidgetId, WindowId); 16]>,

    time_start: Instant,
    time_updates: Vec<(Instant, WidgetId, u64)>,
    // TODO(opt): consider other containers, e.g. C++ multimap
    // or sorted Vec with binary search yielding a range
    handle_updates: HashMap<UpdateHandle, LinearSet<WidgetId>>,
    pending: SmallVec<[Pending; 8]>,
    action: TkAction,
}

/// internals
impl ManagerState {
    fn set_pan_on(
        &mut self,
        id: WidgetId,
        mode: GrabMode,
        source_is_touch: bool,
        coord: Coord,
    ) -> (u16, u16) {
        for (gi, grab) in self.pan_grab.iter_mut().enumerate() {
            if grab.id == id {
                if grab.source_is_touch != source_is_touch {
                    self.remove_pan(gi);
                    break;
                }

                let index = grab.n;
                if usize::from(index) < MAX_PAN_GRABS {
                    grab.coords[usize::from(index)] = (coord, coord);
                }
                grab.n = index + 1;
                return (gi.cast(), index);
            }
        }

        let gj = self.pan_grab.len().cast();
        let n = 1;
        let mut coords: [(Coord, Coord); MAX_PAN_GRABS] = Default::default();
        coords[0] = (coord, coord);
        trace!("Manager: start pan grab {} on {}", self.pan_grab.len(), id);
        self.pan_grab.push(PanGrab {
            id,
            mode,
            source_is_touch,
            n,
            coords,
        });
        (gj, 0)
    }

    fn remove_pan(&mut self, index: usize) {
        trace!("Manager: end pan grab {}", index);
        self.pan_grab.remove(index);
        if let Some(grab) = &mut self.mouse_grab {
            let p0 = grab.pan_grab.0;
            if usize::from(p0) >= index && p0 != u16::MAX {
                grab.pan_grab.0 = p0 - 1;
            }
        }
        for grab in self.touch_grab.iter_mut() {
            let p0 = grab.1.pan_grab.0;
            if usize::from(p0) >= index && p0 != u16::MAX {
                grab.1.pan_grab.0 = p0 - 1;
            }
        }
    }

    fn remove_pan_grab(&mut self, g: (u16, u16)) {
        if let Some(grab) = self.pan_grab.get_mut(usize::from(g.0)) {
            grab.n -= 1;
            if grab.n == 0 {
                return self.remove_pan(g.0.into());
            }
            assert!(grab.source_is_touch);
            for i in (usize::from(g.1))..(usize::from(grab.n) - 1) {
                grab.coords[i] = grab.coords[i + 1];
            }
        } else {
            return; // shouldn't happen
        }

        // Note: the fact that grab.n > 0 implies source is a touch event!
        for grab in self.touch_grab.iter_mut() {
            let grab = grab.1;
            if grab.pan_grab.0 == g.0 && grab.pan_grab.1 > g.1 {
                grab.pan_grab.1 -= 1;
                if usize::from(grab.pan_grab.1) == MAX_PAN_GRABS - 1 {
                    let v = grab.coord.into();
                    self.pan_grab[usize::from(g.0)].coords[usize::from(grab.pan_grab.1)] = (v, v);
                }
            }
        }
    }
}

/// Manager of event-handling and toolkit actions
///
/// A `Manager` is in fact a handle around [`ManagerState`] and [`ShellWindow`]
/// in order to provide a convenient user-interface during event processing.
///
/// It exposes two interfaces: one aimed at users implementing widgets and UIs
/// and one aimed at shells. The latter is hidden
/// from documentation unless the `internal_doc` feature is enabled.
#[must_use]
pub struct Manager<'a> {
    read_only: bool,
    state: &'a mut ManagerState,
    shell: &'a mut dyn ShellWindow,
    action: TkAction,
}

/// Internal methods
impl<'a> Manager<'a> {
    fn set_hover<W: Widget + ?Sized>(&mut self, widget: &W, w_id: Option<WidgetId>) {
        if self.state.hover != w_id {
            trace!("Manager: hover = {:?}", w_id);
            if let Some(id) = self.state.hover {
                if widget
                    .find_leaf(id)
                    .map(|w| w.hover_highlight())
                    .unwrap_or(false)
                {
                    self.redraw(id);
                }
            }
            if let Some(id) = w_id {
                if widget
                    .find_leaf(id)
                    .map(|w| w.hover_highlight())
                    .unwrap_or(false)
                {
                    self.redraw(id);
                }
            }
            self.state.hover = w_id;

            if let Some(id) = w_id {
                let mut icon = widget.cursor_icon();
                let mut widget = widget.as_widget();
                while let Some(child) = widget.find_child(id) {
                    widget = widget.get_child(child).unwrap();
                    let child_icon = widget.cursor_icon();
                    if child_icon != CursorIcon::Default {
                        icon = child_icon;
                    }
                }
                if icon != self.state.hover_icon {
                    self.state.hover_icon = icon;
                    if self.state.mouse_grab.is_none() {
                        self.shell.set_cursor_icon(icon);
                    }
                }
            }
        }
    }

    fn start_key_event<W>(&mut self, widget: &mut W, vkey: VirtualKeyCode, scancode: u32)
    where
        W: Widget<Msg = VoidMsg> + ?Sized,
    {
        use VirtualKeyCode as VK;
        let config = self.state.config.borrow();
        let opt_command = config.shortcuts.get(self.state.modifiers, vkey);
        drop(config);
        let shift = self.state.modifiers.shift();

        if self.state.char_focus {
            if let Some(id) = self.state.sel_focus {
                if let Some(cmd) = opt_command {
                    let event = Event::Command(cmd, shift);
                    trace!("Send to {}: {:?}", id, event);
                    match widget.send(self, id, event) {
                        Response::Unhandled => match cmd {
                            Command::Escape => self.set_char_focus(None),
                            _ => (),
                        },
                        _ => (),
                    }
                }
                return;
            }
        }

        if vkey == VK::Tab {
            if !self.next_nav_focus(widget.as_widget(), shift) {
                self.clear_nav_focus();
            }
            if let Some(id) = self.state.nav_focus {
                self.send_event(widget, id, Event::NavFocus);
            }
            return;
        }

        let mut id_action = None;
        if !self.state.modifiers.alt() {
            // First priority goes to the widget with nav focus,
            // but only when Alt is not pressed.
            if let Some(nav_id) = self.state.nav_focus {
                if vkey == VK::Space || vkey == VK::Return || vkey == VK::NumpadEnter {
                    id_action = Some((nav_id, Event::Activate));
                } else if let Some(cmd) = opt_command {
                    id_action = Some((nav_id, Event::Command(cmd, shift)));
                }
            }

            if id_action.is_none() {
                // Next priority goes to pop-up widget
                if let Some(cmd) = opt_command {
                    let ev = Event::Command(cmd, shift);
                    if let Some(popup) = self.state.popups.last() {
                        id_action = Some((popup.1.parent, ev));
                    } else if let Some(id) = self.state.nav_fallback {
                        id_action = Some((id, ev));
                    }
                }
            }
        }

        if id_action.is_none() {
            // Next priority goes to accelerator keys when Alt is held or alt_bypass is true
            let mut n = 0;
            for (i, id) in (self.state.popups.iter().rev())
                .map(|(_, popup)| popup.parent)
                .chain(std::iter::once(widget.id()))
                .enumerate()
            {
                if let Some(layer) = self.state.accel_layers.get(&id) {
                    // but only when Alt is held or alt-bypass is enabled:
                    if self.state.modifiers.alt() || layer.0 {
                        if let Some(id) = layer.1.get(&vkey).cloned() {
                            id_action = Some((id, Event::Activate));
                            n = i;
                            break;
                        }
                    }
                }
            }

            // If we had to look below the top pop-up, we should close it
            if n > 0 {
                let last = self.state.popups.len() - 1;
                for i in 0..n {
                    let id = self.state.popups[last - i].0;
                    self.close_window(id);
                }
            }
        }

        if let Some((id, event)) = id_action {
            let is_activate = event == Event::Activate;
            trace!("Send to {}: {:?}", id, event);
            match widget.send(self, id, event) {
                Response::Unhandled if vkey == VK::Escape => {
                    // When unhandled, the Escape key causes other actions
                    if let Some(id) = self.state.popups.last().map(|(id, _)| *id) {
                        self.close_window(id);
                    } else if self.nav_focus().is_some() {
                        self.clear_nav_focus();
                    }
                }
                _ => (),
            }

            // Event::Activate causes buttons to be visually depressed
            if is_activate {
                for press_id in self.state.key_depress.values().cloned() {
                    if press_id == id {
                        return;
                    }
                }

                self.state.key_depress.insert(scancode, id);
                self.redraw(id);
            }
        }
    }

    fn end_key_event(&mut self, scancode: u32) {
        // We must match scancode not vkey since the latter may have changed due to modifiers
        if let Some(id) = self.state.key_depress.remove(&scancode) {
            self.redraw(id);
        }
    }

    fn mouse_grab(&self) -> Option<MouseGrab> {
        self.state.mouse_grab.clone()
    }

    fn end_mouse_grab(&mut self, button: MouseButton) {
        if self
            .state
            .mouse_grab
            .as_ref()
            .map(|grab| grab.button != button)
            .unwrap_or(true)
        {
            return;
        }
        if let Some(grab) = self.state.mouse_grab.take() {
            trace!("Manager: end mouse grab by {}", grab.start_id);
            self.shell.set_cursor_icon(self.state.hover_icon);
            self.redraw(grab.start_id);
            self.state.remove_pan_grab(grab.pan_grab);
        }
    }

    #[inline]
    fn get_touch(&mut self, touch_id: u64) -> Option<&mut TouchGrab> {
        self.state.touch_grab.get_mut(&touch_id)
    }

    fn remove_touch(&mut self, touch_id: u64) -> Option<TouchGrab> {
        self.state.touch_grab.remove(&touch_id).map(|grab| {
            trace!("Manager: end touch grab by {}", grab.start_id);
            grab
        })
    }

    fn set_char_focus(&mut self, wid: Option<WidgetId>) {
        trace!(
            "Manager::set_char_focus: char_focus={:?}, new={:?}",
            self.state.char_focus,
            wid
        );

        if let Some(id) = wid {
            self.set_nav_focus(id);
        }

        if self.state.sel_focus == wid {
            // We cannot lose char focus here
            // Corner case: char_focus == true but sel_focus == None: ignore char_focus
            self.state.char_focus = wid.is_some();
            return;
        }

        let had_char_focus = self.state.char_focus;
        self.state.char_focus = wid.is_some();

        if let Some(id) = self.state.sel_focus {
            debug_assert!(Some(id) != wid);

            if had_char_focus {
                // If widget has char focus, this is lost
                self.state.pending.push(Pending::LostCharFocus(id));
            }

            if wid.is_none() {
                return;
            }

            // Selection focus is lost if another widget receives char focus
            self.state.pending.push(Pending::LostSelFocus(id));
        }

        if let Some(id) = wid {
            self.state.sel_focus = Some(id);
        }
    }

    fn send_event<W: Widget + ?Sized>(&mut self, widget: &mut W, id: WidgetId, event: Event) {
        trace!("Send to {}: {:?}", id, event);
        let _ = widget.send(self, id, event);
    }

    fn send_popup_first<W: Widget + ?Sized>(&mut self, widget: &mut W, id: WidgetId, event: Event) {
        while let Some((wid, parent)) = self.state.popups.last().map(|(wid, p)| (*wid, p.parent)) {
            trace!("Send to popup parent: {}: {:?}", parent, event);
            match widget.send(self, parent, event.clone()) {
                Response::Unhandled => (),
                _ => return,
            }
            self.close_window(wid);
        }
        self.send_event(widget, id, event);
    }
}

/// Helper used during widget configuration
pub struct ConfigureManager<'a: 'b, 'b> {
    id: &'b mut WidgetId,
    map: &'b mut HashMap<WidgetId, WidgetId>,
    mgr: &'b mut Manager<'a>,
}

impl<'a: 'b, 'b> ConfigureManager<'a, 'b> {
    /// Reborrow self to pass to a child
    pub fn child<'c>(&'c mut self) -> ConfigureManager<'a, 'c>
    where
        'b: 'c,
    {
        ConfigureManager {
            id: &mut *self.id,
            map: &mut *self.map,
            mgr: &mut *self.mgr,
        }
    }

    /// Get the next [`WidgetId`], without advancing the counter
    pub fn peek_next(&self) -> WidgetId {
        *self.id
    }

    /// Get a new [`WidgetId`] for the widget
    ///
    /// Pass the old ID (`self.id()`), even if not yet configured.
    pub fn next_id(&mut self, old_id: WidgetId) -> WidgetId {
        let id = *self.id;
        *self.id = id.next();
        self.map.insert(old_id, id);
        id
    }

    /// Get access to the wrapped [`Manager`]
    pub fn mgr(&mut self) -> &mut Manager<'a> {
        self.mgr
    }
}
