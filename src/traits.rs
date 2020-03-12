// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Widget traits

use std::fmt;
use std::ops::DerefMut;
use std::time::Duration;

use crate::draw::{DrawHandle, SizeHandle};
use crate::event::{self, Manager, ManagerState};
use crate::geom::{Coord, Rect, Size};
use crate::layout::{self, AxisInfo, SizeRules};
use crate::{AlignHints, CoreData, WidgetId};

/// Support trait for cloning boxed unsized objects
#[cfg_attr(not(feature = "internal_doc"), doc(hidden))]
pub trait CloneTo {
    unsafe fn clone_to(&self, out: *mut Self);
}

impl<T: Clone + Sized> CloneTo for T {
    unsafe fn clone_to(&self, out: *mut Self) {
        let x = self.clone();
        std::ptr::copy(&x, out, 1);
        std::mem::forget(x);
    }
}

/// Base widget functionality
///
/// This trait is almost always implemented via the
/// [`derive(Widget)` macro](macros/index.html#the-derivewidget-macro).
pub trait WidgetCore: fmt::Debug {
    /// Get direct access to the [`CoreData`] providing property storage.
    fn core_data(&self) -> &CoreData;

    /// Get mutable access to the [`CoreData`] providing property storage.
    ///
    /// This should not normally be needed by user code.
    #[cfg_attr(not(feature = "internal_doc"), doc(hidden))]
    fn core_data_mut(&mut self) -> &mut CoreData;

    /// Get the widget's numeric identifier
    #[inline]
    fn id(&self) -> WidgetId {
        self.core_data().id
    }

    /// Get the widget's region, relative to its parent.
    #[inline]
    fn rect(&self) -> Rect {
        self.core_data().rect
    }

    /// Get the name of the widget struct
    fn widget_name(&self) -> &'static str;

    /// Erase type
    fn as_widget(&self) -> &dyn Widget;
    /// Erase type
    fn as_widget_mut(&mut self) -> &mut dyn Widget;

    /// Get the number of child widgets
    fn len(&self) -> usize;

    /// Get a reference to a child widget by index, or `None` if the index is
    /// out of bounds.
    ///
    /// For convenience, `Index<usize>` is implemented via this method.
    ///
    /// Required: `index < self.len()`.
    fn get(&self, index: usize) -> Option<&dyn Widget>;

    /// Mutable variant of get
    ///
    /// Warning: directly adjusting a widget without requiring reconfigure or
    /// redraw may break the UI. If a widget is replaced, a reconfigure **must**
    /// be requested. This can be done via [`Manager::send_action`].
    /// This method may be removed in the future.
    fn get_mut(&mut self, index: usize) -> Option<&mut dyn Widget>;

    /// Find a child widget by identifier
    ///
    /// This requires that the widget tree has already been configured by
    /// [`event::ManagerState::configure`].
    fn find(&self, id: WidgetId) -> Option<&dyn Widget> {
        if id == self.id() {
            return Some(self.as_widget());
        } else if id > self.id() {
            return None;
        }

        for i in 0..self.len() {
            if let Some(w) = self.get(i) {
                if id > w.id() {
                    continue;
                }
                return w.find(id);
            }
            break;
        }
        None
    }

    /// Find a child widget by identifier
    ///
    /// This requires that the widget tree has already been configured by
    /// [`ManagerState::configure`].
    fn find_mut(&mut self, id: WidgetId) -> Option<&mut dyn Widget> {
        if id == self.id() {
            return Some(self.as_widget_mut());
        } else if id > self.id() {
            return None;
        }

        for i in 0..self.len() {
            if self.get(i).map(|w| id > w.id()).unwrap_or(true) {
                continue;
            }
            if let Some(w) = self.get_mut(i) {
                return w.find_mut(id);
            }
            break;
        }
        None
    }

    /// Walk through all widgets, calling `f` once on each.
    ///
    /// This walk is iterative (nonconcurrent), depth-first, and always calls
    /// `f` on self *after* walking through all children.
    fn walk(&self, f: &mut dyn FnMut(&dyn Widget));

    /// Walk through all widgets, calling `f` once on each.
    ///
    /// This walk is iterative (nonconcurrent), depth-first, and always calls
    /// `f` on self *after* walking through all children.
    fn walk_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget));
}

/// Positioning and drawing routines for widgets
///
/// This trait contains methods concerned with positioning of contents, other
/// than those in [`event::Handler`].
pub trait Layout: WidgetCore {
    /// Get size rules for the given axis.
    ///
    /// This method takes `&mut self` to allow local caching of child widget
    /// configuration for future `size_rules` and `set_rect` calls.
    ///
    /// Optionally, this method may set `self.rect().size` to the widget's ideal
    /// size for use by [`Layout::set_rect`] when setting alignment.
    ///
    /// If operating on one axis and the other is fixed, then the `other`
    /// parameter is used for the fixed dimension. Additionally, one may assume
    /// that `size_rules` has previously been called on the fixed axis with the
    /// current widget configuration.
    fn size_rules(&mut self, size_handle: &mut dyn SizeHandle, axis: AxisInfo) -> SizeRules;

    /// Adjust to the given size.
    ///
    /// For widgets with children, this is usually implemented via the derive
    /// [macro](kas::macros). For non-parent widgets which stretch to fill
    /// available space, the default implementation suffices. For non-parent
    /// widgets which react to alignment, this is a little more complex to
    /// implement, and can be done in one of two ways:
    ///
    /// 1.  Shrinking to ideal area and aligning within available space (e.g.
    ///     `CheckBoxBare` widget)
    /// 2.  Filling available space and applying alignment to contents (e.g.
    ///     `Label` widget)
    ///
    /// One may assume that `size_rules` has been called for each axis with the
    /// current widget configuration.
    #[inline]
    fn set_rect(&mut self, _size_handle: &mut dyn SizeHandle, rect: Rect, _align: AlignHints) {
        self.core_data_mut().rect = rect;
    }

    /// Find a child widget by coordinate
    ///
    /// This is used by the event manager to target the correct widget given an
    /// event from a coordinate source (mouse pointer, touch event).
    /// Widgets may return their own Id over that of children in order to steal
    /// events (e.g. a button using an inner label widget).
    ///
    /// This must not be called before [`Layout::set_rect`].
    ///
    /// In the case of an empty grid cell, the parent widget is returned
    /// (same behaviour as with events addressed by coordinate).
    /// The only case `None` should be expected is when `coord` is outside the
    /// initial widget's region; however this is not guaranteed.
    #[inline]
    fn find_id(&self, _coord: Coord) -> Option<WidgetId> {
        Some(self.id())
    }

    /// Draw a widget
    ///
    /// This method is called to draw each visible widget (and should not
    /// attempt recursion on child widgets).
    fn draw(&self, draw_handle: &mut dyn DrawHandle, mgr: &ManagerState);
}

/// A widget is a UI element.
///
/// Widgets usually occupy space within the UI and are drawable. Widgets may
/// respond to user events. Widgets may have child widgets.
///
/// Widgets must additionally implement the traits [`WidgetCore`], [`Layout`]
/// and [`event::Handler`]. The
/// [`derive(Widget)` macro](macros/index.html#the-derivewidget-macro) may be
/// used to generate some of these implementations.
pub trait Widget: Layout {
    /// Configure widget
    ///
    /// Widgets are *configured* on window creation and when
    /// [`kas::TkAction::Reconfigure`] is sent.
    ///
    /// This method is called immediately after assigning `self.core_data().id`.
    fn configure(&mut self, _: &mut Manager) {}

    /// Update the widget via a timer
    ///
    /// This method is called on scheduled updates
    /// (see [`Manager::update_on_timer`]).
    ///
    /// When some [`Duration`] is returned, another timed update is scheduled
    /// at approximately this duration from now (but without blocking redraws;
    /// usage of 1ns effectively enables per-frame update with FPS limited via
    /// VSync). Required: `duration > 0`.
    ///
    /// This method being called does not imply a redraw.
    fn update_timer(&mut self, _: &mut Manager) -> Option<Duration> {
        None
    }

    /// Update the widget via an update handle
    ///
    /// This method is called on triggered updates (see [`Manager::update_on_handle`]).
    /// The source handle is specified via the [`event::UpdateHandle`] parameter.
    ///
    /// A user-defined payload is passed. Interpretation of this payload is
    /// user-defined and unfortunately not type safe.
    ///
    /// This method being called does not imply a redraw.
    fn update_handle(&mut self, _mgr: &mut Manager, _handle: event::UpdateHandle, _payload: u64) {}

    /// Is this widget navigable via Tab key?
    fn allow_focus(&self) -> bool {
        false
    }

    /// Which cursor icon should be used on hover?
    ///
    /// Where no specific icon should be used, return [`event::CursorIcon::Default`].
    fn cursor_icon(&self) -> event::CursorIcon {
        event::CursorIcon::Default
    }
}

/// Trait to describe the type needed by the layout implementation.
///
/// To allow the `derive(Widget)` macro to implement [`Widget`], we use an
/// associated type to describe a data field of the following form:
/// ```none
/// #[layout_data] layout_data: <Self as kas::LayoutData>::Data,
/// ```
///
/// Ideally we would use an inherent associated type on the struct in question,
/// but until rust-lang#8995 is implemented that is not possible. We also cannot
/// place this associated type on the [`Widget`] trait itself, since then uses
/// of the trait would require parameterisation. Thus, this trait.
pub trait LayoutData {
    type Data: Clone + fmt::Debug + Default;
    type Solver: layout::RulesSolver;
    type Setter: layout::RulesSetter;
}

/// A window is a drawable interactive region provided by windowing system.
// TODO: should this be a trait, instead of simply a struct? Should it be
// implemented by dialogs? Note that from the toolkit perspective, it seems a
// Window should be a Widget. So alternatives are (1) use a struct instead of a
// trait or (2) allow any Widget to derive Window (i.e. implement required
// functionality with macros instead of the generic code below).
pub trait Window: Widget + event::Handler<Msg = event::VoidMsg> {
    /// Get the window title
    fn title(&self) -> &str;

    /// Calculate required size
    ///
    /// Returns optional minimum size, and ideal size.
    fn find_size(&mut self, size_handle: &mut dyn SizeHandle) -> (Option<Size>, Size);

    /// Adjust the size of the window, repositioning widgets.
    ///
    /// Returns optional minimum size and optional maximum size.
    fn resize(
        &mut self,
        size_handle: &mut dyn SizeHandle,
        size: Size,
    ) -> (Option<Size>, Option<Size>);

    /// Get a list of available callbacks.
    ///
    /// This returns a sequence of `(index, condition)` values. The toolkit
    /// should call `trigger_callback(index, mgr)` whenever the condition is met.
    fn callbacks(&self) -> Vec<(usize, event::Callback)>;

    /// Trigger a callback (see `iter_callbacks`).
    fn trigger_callback(&mut self, index: usize, mgr: &mut Manager);
}

/// Return value of [`ThemeApi`] functions
///
/// This type is used to notify the toolkit of required updates.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum ThemeAction {
    /// No action needed
    #[cfg_attr(not(feature = "internal_doc"), doc(hidden))]
    None,
    /// All windows require redrawing
    #[cfg_attr(not(feature = "internal_doc"), doc(hidden))]
    RedrawAll,
    /// Theme sizes have changed
    ///
    /// This implies that per-window theme data must be updated
    /// (via [`kas-theme::Theme::update_window`]) and all widgets resized.
    #[cfg_attr(not(feature = "internal_doc"), doc(hidden))]
    ThemeResize,
}

/// Interface through which a theme can be adjusted at run-time
///
/// All methods return a [`ThemeAction`] to enable correct action when a theme
/// is updated via [`Manager::adjust_theme`]. When adjusting a theme before
/// the UI is started, this return value can be safely ignored.
pub trait ThemeApi {
    /// Set font size. Default is 18. Units are unknown.
    fn set_font_size(&mut self, size: f32) -> ThemeAction;

    /// Change the colour scheme
    ///
    /// If no theme by this name is found, the theme is unchanged.
    // TODO: revise scheme identification and error handling?
    fn set_colours(&mut self, _scheme: &str) -> ThemeAction;

    /// Change the theme itself
    ///
    /// Themes may do nothing, or may react according to their own
    /// interpretation of this method.
    fn set_theme(&mut self, _theme: &str) -> ThemeAction {
        ThemeAction::None
    }
}

impl<T: ThemeApi> ThemeApi for Box<T> {
    fn set_font_size(&mut self, size: f32) -> ThemeAction {
        self.deref_mut().set_font_size(size)
    }
    fn set_colours(&mut self, scheme: &str) -> ThemeAction {
        self.deref_mut().set_colours(scheme)
    }
    fn set_theme(&mut self, theme: &str) -> ThemeAction {
        self.deref_mut().set_theme(theme)
    }
}
