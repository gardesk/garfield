//! Modal dialog component for confirmations.

use anyhow::Result;
use gartk_core::{Key, Point, Rect};
use gartk_render::{Renderer, TextStyle};

/// Dialog button type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogButton {
    Ok,
    Cancel,
    Yes,
    No,
}

/// Dialog result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResult {
    Confirmed,
    Cancelled,
}

/// A modal confirmation dialog.
pub struct ConfirmDialog {
    /// Window bounds (for centering).
    bounds: Rect,
    /// Dialog title.
    title: String,
    /// Dialog message.
    message: String,
    /// Confirm button label.
    confirm_label: String,
    /// Cancel button label.
    cancel_label: String,
    /// Whether the dialog is visible.
    visible: bool,
    /// Currently focused button (0 = confirm, 1 = cancel).
    focused_button: usize,
    /// Hovered button (if any).
    hovered_button: Option<usize>,
}

impl ConfirmDialog {
    /// Create a new confirmation dialog.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            title: String::new(),
            message: String::new(),
            confirm_label: "OK".to_string(),
            cancel_label: "Cancel".to_string(),
            visible: false,
            focused_button: 1, // Default focus on Cancel (safer)
            hovered_button: None,
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Show the dialog with the given configuration.
    pub fn show(&mut self, title: &str, message: &str, confirm_label: &str, cancel_label: &str) {
        self.title = title.to_string();
        self.message = message.to_string();
        self.confirm_label = confirm_label.to_string();
        self.cancel_label = cancel_label.to_string();
        self.visible = true;
        self.focused_button = 1; // Default to cancel
        self.hovered_button = None;
    }

    /// Show a delete confirmation dialog.
    pub fn show_delete_confirm(&mut self, count: usize) {
        let title = "Confirm Delete";
        let message = if count == 1 {
            "Are you sure you want to permanently delete this item?\nThis action cannot be undone.".to_string()
        } else {
            format!("Are you sure you want to permanently delete {} items?\nThis action cannot be undone.", count)
        };
        self.show(title, &message, "Delete", "Cancel");
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Handle key press. Returns Some(result) if dialog should close.
    pub fn handle_key(&mut self, key: &Key) -> Option<DialogResult> {
        if !self.visible {
            return None;
        }

        match key {
            Key::Escape => {
                self.hide();
                Some(DialogResult::Cancelled)
            }
            Key::Return => {
                self.hide();
                if self.focused_button == 0 {
                    Some(DialogResult::Confirmed)
                } else {
                    Some(DialogResult::Cancelled)
                }
            }
            Key::Tab | Key::Left | Key::Right => {
                // Toggle between buttons
                self.focused_button = 1 - self.focused_button;
                None
            }
            _ => None,
        }
    }

    /// Handle mouse move.
    pub fn on_mouse_move(&mut self, pos: Point) {
        if !self.visible {
            return;
        }

        let (confirm_rect, cancel_rect) = self.button_rects();
        if confirm_rect.contains_point(pos) {
            self.hovered_button = Some(0);
        } else if cancel_rect.contains_point(pos) {
            self.hovered_button = Some(1);
        } else {
            self.hovered_button = None;
        }
    }

    /// Handle click. Returns Some(result) if a button was clicked.
    pub fn on_click(&mut self, pos: Point) -> Option<DialogResult> {
        if !self.visible {
            return None;
        }

        let dialog_rect = self.dialog_rect();

        // Click outside dialog closes it
        if !dialog_rect.contains_point(pos) {
            self.hide();
            return Some(DialogResult::Cancelled);
        }

        let (confirm_rect, cancel_rect) = self.button_rects();

        if confirm_rect.contains_point(pos) {
            self.hide();
            return Some(DialogResult::Confirmed);
        }

        if cancel_rect.contains_point(pos) {
            self.hide();
            return Some(DialogResult::Cancelled);
        }

        None
    }

    /// Get the dialog rectangle (centered in bounds).
    fn dialog_rect(&self) -> Rect {
        let dialog_width = 400.min(self.bounds.width.saturating_sub(40));
        let dialog_height = 180.min(self.bounds.height.saturating_sub(40));
        let x = self.bounds.x + (self.bounds.width as i32 - dialog_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - dialog_height as i32) / 2;
        Rect::new(x, y, dialog_width, dialog_height)
    }

    /// Get button rectangles (confirm, cancel).
    fn button_rects(&self) -> (Rect, Rect) {
        let dialog = self.dialog_rect();
        let button_width = 100;
        let button_height = 32;
        let button_y = dialog.y + dialog.height as i32 - button_height as i32 - 16;
        let button_gap = 16;
        let total_width = button_width * 2 + button_gap;
        let start_x = dialog.x + (dialog.width as i32 - total_width as i32) / 2;

        let confirm_rect = Rect::new(start_x, button_y, button_width, button_height);
        let cancel_rect = Rect::new(start_x + button_width as i32 + button_gap as i32, button_y, button_width, button_height);

        (confirm_rect, cancel_rect)
    }

    /// Render the dialog.
    pub fn render(&self, renderer: &Renderer) -> Result<()> {
        if !self.visible {
            return Ok(());
        }

        let theme = renderer.theme();

        // Dim background overlay
        renderer.fill_rect(self.bounds, gartk_core::Color::from_u8(0, 0, 0, 180))?;

        let dialog_rect = self.dialog_rect();

        // Dialog background
        renderer.fill_rounded_rect(dialog_rect, 8.0, theme.background)?;
        renderer.stroke_rounded_rect(dialog_rect, 8.0, theme.border, 1.0)?;

        // Title
        let title_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 2.0)
            .color(theme.foreground);

        renderer.text(
            &self.title,
            (dialog_rect.x + 20) as f64,
            (dialog_rect.y + 20) as f64,
            &title_style,
        )?;

        // Message
        let msg_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);

        // Render message lines
        let mut y = dialog_rect.y + 52;
        for line in self.message.lines() {
            renderer.text(
                line,
                (dialog_rect.x + 20) as f64,
                y as f64,
                &msg_style,
            )?;
            y += (theme.font_size * 1.4) as i32;
        }

        // Buttons
        let (confirm_rect, cancel_rect) = self.button_rects();

        // Confirm button (destructive action - use warning color)
        let confirm_focused = self.focused_button == 0;
        let confirm_hovered = self.hovered_button == Some(0);
        let confirm_bg = if confirm_focused || confirm_hovered {
            gartk_core::Color::from_hex("#c53030").unwrap_or(theme.selection_background)
        } else {
            gartk_core::Color::from_hex("#9b2c2c").unwrap_or(theme.item_background)
        };
        renderer.fill_rounded_rect(confirm_rect, 4.0, confirm_bg)?;
        if confirm_focused {
            renderer.stroke_rounded_rect(confirm_rect, 4.0, theme.foreground, 2.0)?;
        }

        let button_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        let confirm_text_width = renderer.measure_text(&self.confirm_label, &button_style)?.width;
        let confirm_text_x = confirm_rect.x + (confirm_rect.width as i32 - confirm_text_width as i32) / 2;
        let button_text_y = confirm_rect.y + (confirm_rect.height as i32 - theme.font_size as i32) / 2;
        renderer.text(&self.confirm_label, confirm_text_x as f64, button_text_y as f64, &button_style)?;

        // Cancel button
        let cancel_focused = self.focused_button == 1;
        let cancel_hovered = self.hovered_button == Some(1);
        let cancel_bg = if cancel_focused || cancel_hovered {
            theme.item_hover_background
        } else {
            theme.item_background
        };
        renderer.fill_rounded_rect(cancel_rect, 4.0, cancel_bg)?;
        if cancel_focused {
            renderer.stroke_rounded_rect(cancel_rect, 4.0, theme.foreground, 2.0)?;
        }

        let cancel_text_width = renderer.measure_text(&self.cancel_label, &button_style)?.width;
        let cancel_text_x = cancel_rect.x + (cancel_rect.width as i32 - cancel_text_width as i32) / 2;
        renderer.text(&self.cancel_label, cancel_text_x as f64, button_text_y as f64, &button_style)?;

        Ok(())
    }
}
