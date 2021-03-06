// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Progress bar

use std::fmt::Debug;

use kas::prelude::*;

/// A progress bar
///
/// The "progress" value may range from 0.0 to 1.0.
#[derive(Clone, Debug, Default, Widget)]
pub struct ProgressBar<D: Directional> {
    #[widget_core]
    core: CoreData,
    direction: D,
    value: f32,
}

impl<D: Directional + Default> ProgressBar<D> {
    /// Construct a progress bar
    ///
    /// The initial value is `0.0`; use `ProgressBar::with_value` to override.
    #[inline]
    pub fn new() -> Self {
        ProgressBar::new_with_direction(D::default())
    }
}

impl<D: Directional> ProgressBar<D> {
    /// Construct a slider with the given `direction`
    ///
    /// The initial value is `0.0`; use `ProgressBar::with_value` to override.
    #[inline]
    pub fn new_with_direction(direction: D) -> Self {
        ProgressBar {
            core: Default::default(),
            direction,
            value: 0.0,
        }
    }

    /// Set the initial value
    #[inline]
    pub fn with_value(mut self, value: f32) -> Self {
        self.value = value.max(0.0).min(1.0);
        self
    }

    /// Get the current value
    #[inline]
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Set the value
    ///
    /// Returns [`TkAction::REDRAW`] if a redraw is required.
    pub fn set_value(&mut self, value: f32) -> TkAction {
        let value = value.max(0.0).min(1.0);
        if value == self.value {
            TkAction::empty()
        } else {
            self.value = value;
            TkAction::REDRAW
        }
    }
}

impl<D: Directional> Layout for ProgressBar<D> {
    fn size_rules(&mut self, size_handle: &mut dyn SizeHandle, axis: AxisInfo) -> SizeRules {
        let mut size = size_handle.progress_bar();
        if self.direction.is_vertical() {
            size = size.transpose();
        }
        let margins = (0, 0);
        if self.direction.is_vertical() == axis.is_vertical() {
            SizeRules::new(size.0, size.0, margins, Stretch::High)
        } else {
            SizeRules::fixed(size.1, margins)
        }
    }

    fn draw(&self, draw_handle: &mut dyn DrawHandle, mgr: &ManagerState, disabled: bool) {
        let dir = self.direction.as_direction();
        let state = self.input_state(mgr, disabled);
        draw_handle.progress_bar(self.core.rect, dir, state, self.value);
    }
}
