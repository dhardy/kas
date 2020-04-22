// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Menu Entries

use std::fmt::{self, Debug};

use kas::class::{HasBool, HasText};
use kas::draw::{DrawHandle, SizeHandle, TextClass};
use kas::event::{Event, Manager, Response, VoidMsg};
use kas::layout::{AxisInfo, Margins, RulesSetter, RulesSolver, SizeRules};
use kas::prelude::*;
use kas::widget::{CheckBoxBare, Label};

/// A standard menu entry
#[widget(config(key_nav = true))]
#[handler(handle=noauto)]
#[derive(Clone, Debug, Default, Widget)]
pub struct MenuEntry<M: Clone + Debug> {
    #[widget_core]
    core: kas::CoreData,
    label: CowString,
    label_off: Coord,
    msg: M,
}

impl<M: Clone + Debug> Layout for MenuEntry<M> {
    fn size_rules(&mut self, size_handle: &mut dyn SizeHandle, axis: AxisInfo) -> SizeRules {
        let size = size_handle.menu_frame();
        self.label_off = size.into();
        let frame_rules = SizeRules::extract_fixed(axis.is_vertical(), size + size, Margins::ZERO);
        let text_rules = size_handle.text_bound(&self.label, TextClass::Label, axis);
        text_rules.surrounded_by(frame_rules, true)
    }

    fn draw(&self, draw_handle: &mut dyn DrawHandle, mgr: &event::ManagerState, disabled: bool) {
        draw_handle.menu_entry(self.core.rect, self.input_state(mgr, disabled));
        let rect = Rect {
            pos: self.core.rect.pos + self.label_off,
            size: self.core.rect.size - self.label_off.into(),
        };
        let align = (Align::Begin, Align::Centre);
        draw_handle.text(rect, &self.label, TextClass::Label, align);
    }
}

impl<M: Clone + Debug> MenuEntry<M> {
    /// Construct a menu item with a given `label` and `msg`
    ///
    /// The message `msg` is emitted on activation. Any
    /// type supporting `Clone` is valid, though it is recommended to use a
    /// simple `Copy` type (e.g. an enum).
    pub fn new<S: Into<CowString>>(label: S, msg: M) -> Self {
        MenuEntry {
            core: Default::default(),
            label: label.into(),
            label_off: Coord::ZERO,
            msg,
        }
    }

    /// Replace the message value
    pub fn set_msg(&mut self, msg: M) {
        self.msg = msg;
    }
}

impl<M: Clone + Debug> HasText for MenuEntry<M> {
    fn get_text(&self) -> &str {
        &self.label
    }

    fn set_cow_string(&mut self, text: CowString) -> TkAction {
        self.label = text;
        TkAction::Redraw
    }
}

impl<M: Clone + Debug> event::Handler for MenuEntry<M> {
    type Msg = M;

    fn handle(&mut self, _: &mut Manager, event: Event) -> Response<M> {
        match event {
            Event::Activate => self.msg.clone().into(),
            event => Response::Unhandled(event),
        }
    }
}

/// A menu entry which can be toggled
#[handler(msg = M, generics = <> where M: From<VoidMsg>)]
#[derive(Clone, Default, Widget)]
pub struct MenuToggle<M> {
    #[widget_core]
    core: CoreData,
    layout_data: layout::FixedRowStorage<[SizeRules; 3], [u32; 2]>,
    #[widget]
    checkbox: CheckBoxBare<M>,
    #[widget]
    label: Label,
}

impl<M> Debug for MenuToggle<M> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MenuToggle {{ core: {:?}, layout_data: {:?}, checkbox: {:?}, label: {:?} }}",
            self.core, self.layout_data, self.checkbox, self.label,
        )
    }
}

impl<M> MenuToggle<M> {
    /// Construct a togglable menu entry with a given `label` and closure
    ///
    /// This is a shortcut for `MenuToggle::new(label).on_toggle(f)`.
    /// The closure `f` is called with the new state of the checkbox when
    /// toggled, and the result of `f` is returned from the event handler.
    #[inline]
    pub fn new_on<T: Into<CowString>, F>(f: F, label: T) -> Self
    where
        F: Fn(bool) -> M + 'static,
    {
        MenuToggle {
            core: Default::default(),
            layout_data: Default::default(),
            checkbox: CheckBoxBare::new_on(f),
            label: Label::new(label),
        }
    }

    /// Set the initial state of the checkbox.
    #[inline]
    pub fn state(mut self, state: bool) -> Self {
        self.checkbox = self.checkbox.state(state);
        self
    }
}

impl MenuToggle<VoidMsg> {
    /// Construct a togglable menu entry with a given `label`
    #[inline]
    pub fn new<T: Into<CowString>>(label: T) -> Self {
        MenuToggle {
            core: Default::default(),
            layout_data: Default::default(),
            checkbox: CheckBoxBare::new(),
            label: Label::new(label),
        }
    }

    /// Set the event handler to be called on toggle.
    ///
    /// The closure `f` is called with the new state of the checkbox when
    /// toggled, and the result of `f` is returned from the event handler.
    #[inline]
    pub fn on_toggle<M, F>(self, f: F) -> MenuToggle<M>
    where
        F: Fn(bool) -> M + 'static,
    {
        MenuToggle {
            core: self.core,
            layout_data: self.layout_data,
            checkbox: self.checkbox.on_toggle(f),
            label: self.label,
        }
    }
}

impl<M> kas::Layout for MenuToggle<M> {
    // NOTE: This code is mostly copied from the macro expansion.
    // Only draw() is significantly different.
    fn size_rules(
        &mut self,
        size_handle: &mut dyn SizeHandle,
        axis: AxisInfo,
    ) -> kas::layout::SizeRules {
        let mut solver = layout::RowSolver::new(axis, (kas::Right, 2usize), &mut self.layout_data);
        let child = &mut self.checkbox;
        solver.for_child(&mut self.layout_data, 0usize, |axis| {
            child.size_rules(size_handle, axis)
        });
        let child = &mut self.label;
        solver.for_child(&mut self.layout_data, 1usize, |axis| {
            child.size_rules(size_handle, axis)
        });
        solver.finish(&mut self.layout_data)
    }

    fn set_rect(&mut self, rect: Rect, align: AlignHints) {
        self.core.rect = rect;
        let mut setter = layout::RowSetter::<_, [u32; 2], _>::new(
            rect,
            (kas::Right, 2usize),
            align,
            &mut self.layout_data,
        );
        let align = kas::AlignHints::NONE;
        self.checkbox.set_rect(
            setter.child_rect(&mut self.layout_data, 0usize),
            align.clone(),
        );
        self.label
            .set_rect(setter.child_rect(&mut self.layout_data, 1usize), align);
    }

    fn find_id(&self, coord: Coord) -> Option<WidgetId> {
        if !self.rect().contains(coord) {
            return None;
        }
        Some(self.checkbox.id())
    }

    fn draw(&self, draw_handle: &mut dyn DrawHandle, mgr: &event::ManagerState, disabled: bool) {
        let state = self.checkbox.input_state(mgr, disabled);
        draw_handle.menu_entry(self.core.rect, state);
        self.checkbox.draw(draw_handle, mgr, state.disabled);
        self.label.draw(draw_handle, mgr, state.disabled);
    }
}
impl<M> HasBool for MenuToggle<M> {
    #[inline]
    fn get_bool(&self) -> bool {
        self.checkbox.get_bool()
    }

    #[inline]
    fn set_bool(&mut self, state: bool) -> TkAction {
        self.checkbox.set_bool(state)
    }
}
