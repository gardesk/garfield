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

/// Padding from edges.
const PADDING: i32 = 8;

/// Gap between buttons.
const BUTTON_GAP: i32 = 12;

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
    /// Filter text bounds (for hover detection).
    filter_bounds: Rect,
    /// Hovered button (0 = accept, 1 = cancel).
    hovered: Option<usize>,
    /// Whether filter text is hovered.
    filter_hovered: bool,
    /// Focused button for keyboard navigation (0 = accept, 1 = cancel).
    focused: usize,
    /// Whether accept button is enabled (has valid selection).
    accept_enabled: bool,
    /// Filter description shown in toolbar (full text).
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
            filter_bounds: Rect::default(),
            hovered: None,
            filter_hovered: false,
            focused: 0,
            accept_enabled: false,
            filter_description: None,
        };
        toolbar.layout();
        toolbar
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.layout();
    }

    /// Layout buttons and filter area.
    fn layout(&mut self) {
        // Cancel button (rightmost)
        let cancel_x = self.bounds.x + self.bounds.width as i32 - BUTTON_WIDTH as i32 - PADDING;
        let button_y = self.bounds.y + (self.bounds.height as i32 - BUTTON_HEIGHT as i32) / 2;
        self.cancel_bounds = Rect::new(cancel_x, button_y, BUTTON_WIDTH, BUTTON_HEIGHT);

        // Accept button (to the left of cancel)
        let accept_x = cancel_x - BUTTON_WIDTH as i32 - BUTTON_GAP;
        self.accept_bounds = Rect::new(accept_x, button_y, BUTTON_WIDTH, BUTTON_HEIGHT);

        // Filter text area (left side, up to accept button)
        let filter_x = self.bounds.x + PADDING;
        let filter_width = (accept_x - BUTTON_GAP - filter_x).max(0) as u32;
        self.filter_bounds = Rect::new(filter_x, self.bounds.y, filter_width, self.bounds.height);
    }

    /// Get max width available for filter text.
    fn max_filter_width(&self) -> u32 {
        self.filter_bounds.width.saturating_sub(8) // Small padding
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
        let mut changed = false;

        if !self.bounds.contains_point(pos) {
            if self.hovered.is_some() {
                self.hovered = None;
                changed = true;
            }
            if self.filter_hovered {
                self.filter_hovered = false;
                changed = true;
            }
            return changed;
        }

        let new_hovered = if self.accept_bounds.contains_point(pos) {
            Some(0)
        } else if self.cancel_bounds.contains_point(pos) {
            Some(1)
        } else {
            None
        };

        if new_hovered != self.hovered {
            self.hovered = new_hovered;
            changed = true;
        }

        // Check if hovering over filter text area
        let new_filter_hovered = self.filter_bounds.contains_point(pos) && self.filter_description.is_some();
        if new_filter_hovered != self.filter_hovered {
            self.filter_hovered = new_filter_hovered;
            changed = true;
        }

        changed
    }

    /// Get tooltip text if hovering over truncated filter.
    pub fn get_tooltip(&self) -> Option<&str> {
        if self.filter_hovered {
            self.filter_description.as_deref()
        } else {
            None
        }
    }

    /// Check if filter is currently hovered.
    pub fn is_filter_hovered(&self) -> bool {
        self.filter_hovered
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

        // Filter description (left side, truncated with ellipsis)
        if let Some(desc) = &self.filter_description {
            let text_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size - 1.0)
                .color(if self.filter_hovered { theme.foreground } else { theme.item_foreground });

            let max_width = self.max_filter_width() as f64;
            let display_text = self.truncate_with_ellipsis(desc, max_width, renderer, &text_style)?;

            let text_x = (self.bounds.x + PADDING + 4) as f64;
            let text_y = (self.bounds.y + (self.bounds.height as i32 - theme.font_size as i32) / 2) as f64;
            renderer.text(&display_text, text_x, text_y, &text_style)?;

            // Show tooltip if hovered
            if self.filter_hovered {
                self.render_tooltip(renderer, desc)?;
            }
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

    /// Truncate text with ellipsis if it exceeds max width.
    fn truncate_with_ellipsis(&self, text: &str, max_width: f64, renderer: &Renderer, style: &TextStyle) -> Result<String> {
        let metrics = renderer.measure_text(text, style)?;
        let max_width = max_width as u32;
        if metrics.width <= max_width {
            return Ok(text.to_string());
        }

        // Need to truncate - binary search for the right length
        let ellipsis = "...";
        let ellipsis_width = renderer.measure_text(ellipsis, style)?.width;
        let target_width = max_width.saturating_sub(ellipsis_width);

        if target_width == 0 {
            return Ok(ellipsis.to_string());
        }

        // Find the longest prefix that fits
        let mut end = text.len();
        for (i, _) in text.char_indices().rev() {
            let prefix = &text[..i];
            let prefix_width = renderer.measure_text(prefix, style)?.width;
            if prefix_width <= target_width {
                end = i;
                break;
            }
        }

        if end == 0 {
            Ok(ellipsis.to_string())
        } else {
            Ok(format!("{}{}", &text[..end], ellipsis))
        }
    }

    /// Render tooltip showing full filter text as multiline.
    fn render_tooltip(&self, renderer: &Renderer, text: &str) -> Result<()> {
        let theme = renderer.theme();

        let tooltip_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 2.0)
            .color(theme.foreground);

        // Parse filter patterns and format as multiline
        // Remove "Filter: " prefix if present
        let filter_text = text.strip_prefix("Filter: ").unwrap_or(text);

        // Split by semicolon or comma and clean up patterns
        let patterns: Vec<&str> = filter_text
            .split(|c| c == ';' || c == ',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        // Format into columns (4 patterns per row max)
        let cols = 4;
        let mut lines: Vec<String> = Vec::new();
        lines.push("Accepted file types:".to_string());

        for chunk in patterns.chunks(cols) {
            let line = chunk.join("  ");
            lines.push(line);
        }

        // Measure dimensions
        let padding = 10u32;
        let line_height = (theme.font_size - 2.0) as u32 + 4;
        let mut max_width = 0u32;

        for line in &lines {
            let metrics = renderer.measure_text(line, &tooltip_style)?;
            max_width = max_width.max(metrics.width);
        }

        let tooltip_width = max_width + padding * 2;
        let tooltip_height = (lines.len() as u32 * line_height) + padding * 2;

        // Position tooltip above the filter area, but keep on screen
        let tooltip_x = self.filter_bounds.x.max(4);
        let tooltip_y = self.bounds.y - tooltip_height as i32 - 4;

        let tooltip_bounds = Rect::new(tooltip_x, tooltip_y, tooltip_width, tooltip_height);

        // Background with border and shadow effect
        let shadow_bounds = Rect::new(tooltip_x + 2, tooltip_y + 2, tooltip_width, tooltip_height);
        renderer.fill_rounded_rect(shadow_bounds, 6.0, theme.background.with_alpha(0.3))?;
        renderer.fill_rounded_rect(tooltip_bounds, 6.0, theme.background)?;
        renderer.stroke_rounded_rect(tooltip_bounds, 6.0, theme.border, 1.0)?;

        // Render each line
        let mut y = tooltip_y + padding as i32;
        for (i, line) in lines.iter().enumerate() {
            let style = if i == 0 {
                // Header line slightly brighter
                TextStyle::new()
                    .font_family(&theme.font_family)
                    .font_size(theme.font_size - 2.0)
                    .color(theme.foreground)
            } else {
                tooltip_style.clone()
            };

            renderer.text(
                line,
                (tooltip_x + padding as i32) as f64,
                y as f64,
                &style,
            )?;
            y += line_height as i32;
        }

        Ok(())
    }
}
