//! Picker toolbar component with Accept/Cancel buttons.
//!
//! This toolbar replaces the normal toolbar when garfield runs in picker mode.

use anyhow::Result;
use gartk_core::{Point, Rect};
use gartk_render::{Renderer, TextStyle};

/// Height of the picker toolbar (same as normal toolbar).
pub const PICKER_TOOLBAR_HEIGHT: u32 = 36;

/// Button width.
const BUTTON_WIDTH: u32 = 100;

/// Button height.
const BUTTON_HEIGHT: u32 = 28;

/// Picker toolbar click result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerToolbarClick {
    /// Accept button clicked.
    Accept,
    /// Cancel button clicked.
    Cancel,
    /// Nothing clicked.
    None,
}

/// Picker toolbar with Accept and Cancel buttons.
pub struct PickerToolbar {
    /// Toolbar bounds.
    bounds: Rect,
    /// Accept button label.
    accept_label: String,
    /// Cancel button label.
    cancel_label: String,
    /// Accept button bounds.
    accept_bounds: Rect,
    /// Cancel button bounds.
    cancel_bounds: Rect,
    /// Hovered button (0 = accept, 1 = cancel).
    hovered: Option<usize>,
    /// Focused button for keyboard navigation (0 = accept, 1 = cancel).
    focused: usize,
    /// Whether accept button is enabled (has valid selection).
    accept_enabled: bool,
    /// Filter description shown in toolbar.
    filter_description: Option<String>,
}

impl PickerToolbar {
    /// Create a new picker toolbar.
    pub fn new(bounds: Rect, accept_label: String) -> Self {
        let mut toolbar = Self {
            bounds,
            accept_label,
            cancel_label: "Cancel".to_string(),
            accept_bounds: Rect::default(),
            cancel_bounds: Rect::default(),
            hovered: None,
            focused: 0,
            accept_enabled: false,
            filter_description: None,
        };
        toolbar.layout_buttons();
        toolbar
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.layout_buttons();
    }

    /// Layout buttons.
    fn layout_buttons(&mut self) {
        // Buttons are right-aligned
        let padding = 8;
        let button_gap = 12;

        // Cancel button (rightmost)
        let cancel_x = self.bounds.x + self.bounds.width as i32 - BUTTON_WIDTH as i32 - padding;
        let button_y = self.bounds.y + (self.bounds.height as i32 - BUTTON_HEIGHT as i32) / 2;
        self.cancel_bounds = Rect::new(cancel_x, button_y, BUTTON_WIDTH, BUTTON_HEIGHT);

        // Accept button (to the left of cancel)
        let accept_x = cancel_x - BUTTON_WIDTH as i32 - button_gap;
        self.accept_bounds = Rect::new(accept_x, button_y, BUTTON_WIDTH, BUTTON_HEIGHT);
    }

    /// Set whether accept button is enabled.
    pub fn set_accept_enabled(&mut self, enabled: bool) {
        self.accept_enabled = enabled;
    }

    /// Set filter description shown in toolbar.
    pub fn set_filter_description(&mut self, desc: Option<String>) {
        self.filter_description = desc;
    }

    /// Handle mouse move. Returns true if hovered state changed.
    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        if !self.bounds.contains_point(pos) {
            if self.hovered.is_some() {
                self.hovered = None;
                return true;
            }
            return false;
        }

        let new_hovered = if self.accept_bounds.contains_point(pos) {
            Some(0)
        } else if self.cancel_bounds.contains_point(pos) {
            Some(1)
        } else {
            None
        };

        let changed = new_hovered != self.hovered;
        self.hovered = new_hovered;
        changed
    }

    /// Handle click. Returns the action if a button was clicked.
    pub fn on_click(&self, pos: Point) -> PickerToolbarClick {
        if self.accept_bounds.contains_point(pos) && self.accept_enabled {
            PickerToolbarClick::Accept
        } else if self.cancel_bounds.contains_point(pos) {
            PickerToolbarClick::Cancel
        } else {
            PickerToolbarClick::None
        }
    }

    /// Cycle focus between buttons.
    pub fn cycle_focus(&mut self) {
        self.focused = 1 - self.focused;
    }

    /// Activate focused button.
    pub fn activate_focused(&self) -> PickerToolbarClick {
        if self.focused == 0 && self.accept_enabled {
            PickerToolbarClick::Accept
        } else if self.focused == 1 {
            PickerToolbarClick::Cancel
        } else {
            PickerToolbarClick::None
        }
    }

    /// Check if point is within toolbar bounds.
    pub fn contains_point(&self, pos: Point) -> bool {
        self.bounds.contains_point(pos)
    }

    /// Render the toolbar.
    pub fn render(&self, renderer: &Renderer) -> Result<()> {
        let theme = renderer.theme();

        // Toolbar background
        renderer.fill_rect(self.bounds, theme.item_background)?;

        // Filter description (left side)
        if let Some(desc) = &self.filter_description {
            let text_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size - 1.0)
                .color(theme.item_foreground);

            renderer.text(
                desc,
                (self.bounds.x + 12) as f64,
                (self.bounds.y + (self.bounds.height as i32 - theme.font_size as i32) / 2) as f64,
                &text_style,
            )?;
        }

        // Accept button
        let accept_hovered = self.hovered == Some(0);
        let accept_focused = self.focused == 0;
        let (accept_bg, accept_fg) = if !self.accept_enabled {
            // Disabled state - use input_background for visibility
            (theme.input_background.with_alpha(0.5), theme.item_foreground.with_alpha(0.5))
        } else if accept_hovered || accept_focused {
            // Active state - use accent color
            (theme.selection_background, theme.foreground)
        } else {
            // Normal state - accent color slightly dimmed
            (theme.selection_background.with_alpha(0.8), theme.foreground)
        };

        renderer.fill_rounded_rect(self.accept_bounds, 4.0, accept_bg)?;
        if accept_focused && self.accept_enabled {
            renderer.stroke_rounded_rect(self.accept_bounds, 4.0, theme.foreground, 2.0)?;
        }

        let button_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(accept_fg);

        let accept_metrics = renderer.measure_text(&self.accept_label, &button_style)?;
        let accept_text_x = self.accept_bounds.x + (self.accept_bounds.width as i32 - accept_metrics.width as i32) / 2;
        let accept_text_y = self.accept_bounds.y + (self.accept_bounds.height as i32 - accept_metrics.height as i32) / 2;
        renderer.text(&self.accept_label, accept_text_x as f64, accept_text_y as f64, &button_style)?;

        // Cancel button - use input_background for visibility
        let cancel_hovered = self.hovered == Some(1);
        let cancel_focused = self.focused == 1;
        let cancel_bg = if cancel_hovered || cancel_focused {
            theme.item_hover_background
        } else {
            theme.input_background
        };

        renderer.fill_rounded_rect(self.cancel_bounds, 4.0, cancel_bg)?;
        if cancel_focused {
            renderer.stroke_rounded_rect(self.cancel_bounds, 4.0, theme.foreground, 2.0)?;
        }
        renderer.stroke_rounded_rect(self.cancel_bounds, 4.0, theme.border, 1.0)?;

        let cancel_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        let cancel_metrics = renderer.measure_text(&self.cancel_label, &cancel_style)?;
        let cancel_text_x = self.cancel_bounds.x + (self.cancel_bounds.width as i32 - cancel_metrics.width as i32) / 2;
        let cancel_text_y = self.cancel_bounds.y + (self.cancel_bounds.height as i32 - cancel_metrics.height as i32) / 2;
        renderer.text(&self.cancel_label, cancel_text_x as f64, cancel_text_y as f64, &cancel_style)?;

        // Bottom border
        let border_y = self.bounds.y + self.bounds.height as i32 - 1;
        renderer.fill_rect(
            Rect::new(self.bounds.x, border_y, self.bounds.width, 1),
            theme.border,
        )?;

        Ok(())
    }
}
