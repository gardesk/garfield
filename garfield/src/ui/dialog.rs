//! Modal dialog components for confirmations and progress.

use anyhow::Result;
use gartk_core::{Key, Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::time::Instant;

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
        // Calculate width based on message length, with min/max constraints
        let base_width = 480;
        let dialog_width = base_width.min(self.bounds.width.saturating_sub(20));
        // Need enough height for: title (40) + message (~80) + gap (20) + buttons (32) + padding (48)
        let dialog_height = 240.min(self.bounds.height.saturating_sub(20));
        let x = self.bounds.x + (self.bounds.width as i32 - dialog_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - dialog_height as i32) / 2;
        Rect::new(x, y, dialog_width, dialog_height)
    }

    /// Wrap text to fit within max_width (simple word wrapping).
    fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for line in text.lines() {
            if line.len() <= max_chars {
                lines.push(line.to_string());
            } else {
                // Word wrap
                let mut current_line = String::new();
                for word in line.split_whitespace() {
                    if current_line.is_empty() {
                        if word.len() > max_chars {
                            // Word too long, truncate with ellipsis
                            lines.push(format!("{}...", &word[..max_chars.saturating_sub(3)]));
                        } else {
                            current_line = word.to_string();
                        }
                    } else if current_line.len() + 1 + word.len() <= max_chars {
                        current_line.push(' ');
                        current_line.push_str(word);
                    } else {
                        lines.push(current_line);
                        current_line = word.to_string();
                    }
                }
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
            }
        }
        lines
    }

    /// Get button rectangles (confirm, cancel).
    fn button_rects(&self) -> (Rect, Rect) {
        let dialog = self.dialog_rect();
        let button_width = 100;
        let button_height = 36;
        // Position buttons 24px from bottom for more breathing room
        let button_y = dialog.y + dialog.height as i32 - button_height as i32 - 24;
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

        // Wrap and render message lines (use conservative char width estimate)
        let max_chars = ((dialog_rect.width - 50) as f64 / (theme.font_size * 0.55)) as usize;
        let wrapped_lines = Self::wrap_text(&self.message, max_chars.max(25));
        let mut y = dialog_rect.y + 56;
        for line in wrapped_lines {
            renderer.text(
                &line,
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

/// Progress information for an operation.
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// Current item being processed.
    pub current_item: String,
    /// Current item index (1-based).
    pub current: usize,
    /// Total items.
    pub total: usize,
    /// Bytes processed (for copy/move).
    pub bytes_done: u64,
    /// Total bytes (for copy/move).
    pub bytes_total: u64,
}

impl ProgressInfo {
    /// Create new progress info.
    pub fn new(total: usize) -> Self {
        Self {
            current_item: String::new(),
            current: 0,
            total,
            bytes_done: 0,
            bytes_total: 0,
        }
    }

    /// Get progress as a fraction (0.0 to 1.0).
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.current as f64 / self.total as f64
        }
    }
}

/// A modal progress dialog for long operations.
pub struct ProgressDialog {
    /// Window bounds (for centering).
    bounds: Rect,
    /// Dialog title (operation name).
    title: String,
    /// Whether the dialog is visible.
    visible: bool,
    /// Progress information.
    progress: ProgressInfo,
    /// Whether the operation can be cancelled.
    cancellable: bool,
    /// Whether cancel was requested.
    cancel_requested: bool,
    /// Whether the cancel button is hovered.
    cancel_hovered: bool,
    /// Time when dialog was shown.
    start_time: Option<Instant>,
}

impl ProgressDialog {
    /// Create a new progress dialog.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            title: String::new(),
            visible: false,
            progress: ProgressInfo::new(0),
            cancellable: true,
            cancel_requested: false,
            cancel_hovered: false,
            start_time: None,
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Show the progress dialog.
    pub fn show(&mut self, title: &str, total: usize, cancellable: bool) {
        self.title = title.to_string();
        self.progress = ProgressInfo::new(total);
        self.cancellable = cancellable;
        self.cancel_requested = false;
        self.cancel_hovered = false;
        self.visible = true;
        self.start_time = Some(Instant::now());
    }

    /// Update progress.
    pub fn update(&mut self, current: usize, current_item: &str) {
        self.progress.current = current;
        self.progress.current_item = current_item.to_string();
    }

    /// Update byte progress.
    pub fn update_bytes(&mut self, bytes_done: u64, bytes_total: u64) {
        self.progress.bytes_done = bytes_done;
        self.progress.bytes_total = bytes_total;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if cancel was requested.
    pub fn is_cancel_requested(&self) -> bool {
        self.cancel_requested
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
        self.start_time = None;
    }

    /// Handle key press.
    pub fn handle_key(&mut self, key: &Key) -> bool {
        if !self.visible {
            return false;
        }

        match key {
            Key::Escape if self.cancellable => {
                self.cancel_requested = true;
                true
            }
            _ => true, // Consume all keys while dialog is visible
        }
    }

    /// Handle mouse move.
    pub fn on_mouse_move(&mut self, pos: Point) {
        if !self.visible || !self.cancellable {
            return;
        }

        let cancel_rect = self.cancel_button_rect();
        self.cancel_hovered = cancel_rect.contains_point(pos);
    }

    /// Handle click.
    pub fn on_click(&mut self, pos: Point) -> bool {
        if !self.visible {
            return false;
        }

        if self.cancellable {
            let cancel_rect = self.cancel_button_rect();
            if cancel_rect.contains_point(pos) {
                self.cancel_requested = true;
                return true;
            }
        }

        // Consume click but don't do anything else
        true
    }

    /// Get the dialog rectangle (centered in bounds).
    fn dialog_rect(&self) -> Rect {
        let dialog_width = 450.min(self.bounds.width.saturating_sub(40));
        let dialog_height = 160.min(self.bounds.height.saturating_sub(40));
        let x = self.bounds.x + (self.bounds.width as i32 - dialog_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - dialog_height as i32) / 2;
        Rect::new(x, y, dialog_width, dialog_height)
    }

    /// Get progress bar rectangle.
    fn progress_bar_rect(&self) -> Rect {
        let dialog = self.dialog_rect();
        let bar_width = dialog.width.saturating_sub(40);
        let bar_height = 8;
        let x = dialog.x + 20;
        let y = dialog.y + 80;
        Rect::new(x, y, bar_width, bar_height)
    }

    /// Get cancel button rectangle.
    fn cancel_button_rect(&self) -> Rect {
        let dialog = self.dialog_rect();
        let button_width = 80;
        let button_height = 28;
        let x = dialog.x + (dialog.width as i32 - button_width as i32) / 2;
        let y = dialog.y + dialog.height as i32 - button_height as i32 - 16;
        Rect::new(x, y, button_width, button_height)
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

        // Current item
        let item_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.item_foreground);

        let item_text = if self.progress.current_item.is_empty() {
            "Preparing...".to_string()
        } else {
            // Truncate long paths
            let max_len = 50;
            if self.progress.current_item.len() > max_len {
                format!("...{}", &self.progress.current_item[self.progress.current_item.len() - max_len..])
            } else {
                self.progress.current_item.clone()
            }
        };

        renderer.text(
            &item_text,
            (dialog_rect.x + 20) as f64,
            (dialog_rect.y + 50) as f64,
            &item_style,
        )?;

        // Progress bar background
        let bar_rect = self.progress_bar_rect();
        renderer.fill_rounded_rect(bar_rect, 4.0, theme.item_background)?;

        // Progress bar fill
        let progress_fraction = self.progress.fraction();
        if progress_fraction > 0.0 {
            let fill_width = ((bar_rect.width as f64 * progress_fraction) as u32).max(1);
            let fill_rect = Rect::new(bar_rect.x, bar_rect.y, fill_width, bar_rect.height);
            let progress_color = gartk_core::Color::from_hex("#4a9eff").unwrap_or(theme.selection_background);
            renderer.fill_rounded_rect(fill_rect, 4.0, progress_color)?;
        }

        // Progress text (e.g., "3 of 10")
        let progress_text = format!("{} of {}", self.progress.current, self.progress.total);
        let progress_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.item_foreground);

        let text_width = renderer.measure_text(&progress_text, &progress_style)?.width;
        let text_x = bar_rect.x + (bar_rect.width as i32 - text_width as i32) / 2;
        renderer.text(
            &progress_text,
            text_x as f64,
            (bar_rect.y + bar_rect.height as i32 + 8) as f64,
            &progress_style,
        )?;

        // Cancel button (if cancellable)
        if self.cancellable {
            let cancel_rect = self.cancel_button_rect();
            let cancel_bg = if self.cancel_hovered {
                theme.item_hover_background
            } else {
                theme.item_background
            };
            renderer.fill_rounded_rect(cancel_rect, 4.0, cancel_bg)?;

            let button_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size)
                .color(theme.foreground);

            let cancel_text = "Cancel";
            let cancel_width = renderer.measure_text(cancel_text, &button_style)?.width;
            let cancel_x = cancel_rect.x + (cancel_rect.width as i32 - cancel_width as i32) / 2;
            let cancel_y = cancel_rect.y + (cancel_rect.height as i32 - theme.font_size as i32) / 2;
            renderer.text(cancel_text, cancel_x as f64, cancel_y as f64, &button_style)?;
        }

        Ok(())
    }
}

/// Result of a conflict dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAction {
    /// Replace the existing file.
    Replace,
    /// Skip this file.
    Skip,
    /// Keep both (auto-rename).
    KeepBoth,
    /// Cancel the entire operation.
    Cancel,
}

/// A modal dialog for file conflict resolution.
pub struct ConflictDialog {
    /// Window bounds (for centering).
    bounds: Rect,
    /// Conflicting file name.
    filename: String,
    /// Whether the dialog is visible.
    visible: bool,
    /// Currently focused button (0-3).
    focused_button: usize,
    /// Hovered button.
    hovered_button: Option<usize>,
    /// Apply to all remaining conflicts.
    apply_to_all: bool,
}

impl ConflictDialog {
    /// Create a new conflict dialog.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            filename: String::new(),
            visible: false,
            focused_button: 2, // Default to Keep Both (safest)
            hovered_button: None,
            apply_to_all: false,
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Show the dialog for a conflicting file.
    pub fn show(&mut self, filename: &str) {
        self.filename = filename.to_string();
        self.visible = true;
        self.focused_button = 2; // Default to Keep Both
        self.hovered_button = None;
        self.apply_to_all = false;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if apply to all is set.
    pub fn apply_to_all(&self) -> bool {
        self.apply_to_all
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Handle key press. Returns Some(action) if dialog should close.
    pub fn handle_key(&mut self, key: &Key) -> Option<ConflictAction> {
        if !self.visible {
            return None;
        }

        match key {
            Key::Escape => {
                self.hide();
                Some(ConflictAction::Cancel)
            }
            Key::Return => {
                self.hide();
                Some(match self.focused_button {
                    0 => ConflictAction::Replace,
                    1 => ConflictAction::Skip,
                    2 => ConflictAction::KeepBoth,
                    _ => ConflictAction::Cancel,
                })
            }
            Key::Tab | Key::Right => {
                self.focused_button = (self.focused_button + 1) % 4;
                None
            }
            Key::Left => {
                self.focused_button = if self.focused_button == 0 { 3 } else { self.focused_button - 1 };
                None
            }
            Key::Char('a') | Key::Char('A') => {
                // Toggle apply to all
                self.apply_to_all = !self.apply_to_all;
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

        let buttons = self.button_rects();
        self.hovered_button = buttons.iter().position(|r| r.contains_point(pos));
    }

    /// Handle click. Returns Some(action) if a button was clicked.
    pub fn on_click(&mut self, pos: Point) -> Option<ConflictAction> {
        if !self.visible {
            return None;
        }

        let dialog_rect = self.dialog_rect();
        if !dialog_rect.contains_point(pos) {
            self.hide();
            return Some(ConflictAction::Cancel);
        }

        // Check checkbox
        let checkbox_rect = self.checkbox_rect();
        if checkbox_rect.contains_point(pos) {
            self.apply_to_all = !self.apply_to_all;
            return None;
        }

        let buttons = self.button_rects();
        for (i, rect) in buttons.iter().enumerate() {
            if rect.contains_point(pos) {
                self.hide();
                return Some(match i {
                    0 => ConflictAction::Replace,
                    1 => ConflictAction::Skip,
                    2 => ConflictAction::KeepBoth,
                    _ => ConflictAction::Cancel,
                });
            }
        }

        None
    }

    /// Get the dialog rectangle.
    fn dialog_rect(&self) -> Rect {
        let dialog_width = 420.min(self.bounds.width.saturating_sub(40));
        let dialog_height = 180.min(self.bounds.height.saturating_sub(40));
        let x = self.bounds.x + (self.bounds.width as i32 - dialog_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - dialog_height as i32) / 2;
        Rect::new(x, y, dialog_width, dialog_height)
    }

    /// Get button rectangles [Replace, Skip, Keep Both, Cancel].
    fn button_rects(&self) -> [Rect; 4] {
        let dialog = self.dialog_rect();
        let button_width = 85;
        let button_height = 28;
        let button_y = dialog.y + dialog.height as i32 - button_height as i32 - 16;
        let button_gap = 8;
        let total_width = button_width * 4 + button_gap * 3;
        let start_x = dialog.x + (dialog.width as i32 - total_width as i32) / 2;

        [
            Rect::new(start_x, button_y, button_width as u32, button_height),
            Rect::new(start_x + button_width + button_gap, button_y, button_width as u32, button_height),
            Rect::new(start_x + (button_width + button_gap) * 2, button_y, button_width as u32, button_height),
            Rect::new(start_x + (button_width + button_gap) * 3, button_y, button_width as u32, button_height),
        ]
    }

    /// Get checkbox rectangle.
    fn checkbox_rect(&self) -> Rect {
        let dialog = self.dialog_rect();
        Rect::new(dialog.x + 20, dialog.y + 95, 16, 16)
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
            "File Already Exists",
            (dialog_rect.x + 20) as f64,
            (dialog_rect.y + 20) as f64,
            &title_style,
        )?;

        // Message
        let msg_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);

        let message = format!("\"{}\" already exists in the destination.", self.filename);
        renderer.text(
            &message,
            (dialog_rect.x + 20) as f64,
            (dialog_rect.y + 52) as f64,
            &msg_style,
        )?;

        renderer.text(
            "What would you like to do?",
            (dialog_rect.x + 20) as f64,
            (dialog_rect.y + 72) as f64,
            &msg_style,
        )?;

        // Checkbox for "Apply to all"
        let checkbox_rect = self.checkbox_rect();
        renderer.stroke_rounded_rect(checkbox_rect, 2.0, theme.border, 1.0)?;
        if self.apply_to_all {
            // Draw checkmark
            let cx = checkbox_rect.x as f64 + 3.0;
            let cy = checkbox_rect.y as f64 + 8.0;
            renderer.line(cx, cy, cx + 4.0, cy + 4.0, theme.foreground, 2.0)?;
            renderer.line(cx + 4.0, cy + 4.0, cx + 10.0, cy - 4.0, theme.foreground, 2.0)?;
        }

        let checkbox_label_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.item_foreground);

        renderer.text(
            "Apply to all (A)",
            (checkbox_rect.x + 22) as f64,
            (checkbox_rect.y) as f64,
            &checkbox_label_style,
        )?;

        // Buttons
        let buttons = self.button_rects();
        let labels = ["Replace", "Skip", "Keep Both", "Cancel"];

        let button_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.foreground);

        for (i, (rect, label)) in buttons.iter().zip(labels.iter()).enumerate() {
            let focused = self.focused_button == i;
            let hovered = self.hovered_button == Some(i);

            let bg = if focused || hovered {
                theme.item_hover_background
            } else {
                theme.item_background
            };
            renderer.fill_rounded_rect(*rect, 4.0, bg)?;
            if focused {
                renderer.stroke_rounded_rect(*rect, 4.0, theme.foreground, 2.0)?;
            }

            let text_width = renderer.measure_text(label, &button_style)?.width;
            let text_x = rect.x + (rect.width as i32 - text_width as i32) / 2;
            let text_y = rect.y + (rect.height as i32 - (theme.font_size - 1.0) as i32) / 2;
            renderer.text(label, text_x as f64, text_y as f64, &button_style)?;
        }

        Ok(())
    }
}

/// Result of an input dialog.
#[derive(Debug, Clone)]
pub enum InputResult {
    /// User submitted the input.
    Submitted(String),
    /// User cancelled.
    Cancelled,
}

/// A modal dialog for text input.
pub struct InputDialog {
    /// Window bounds (for centering).
    bounds: Rect,
    /// Dialog title.
    title: String,
    /// Dialog prompt/label.
    prompt: String,
    /// Current input value.
    input: String,
    /// Cursor position in input.
    cursor: usize,
    /// Whether the dialog is visible.
    visible: bool,
    /// Currently focused element (0 = input, 1 = ok, 2 = cancel).
    focused: usize,
    /// Hovered button.
    hovered_button: Option<usize>,
}

impl InputDialog {
    /// Create a new input dialog.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            title: String::new(),
            prompt: String::new(),
            input: String::new(),
            cursor: 0,
            visible: false,
            focused: 0,
            hovered_button: None,
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Show the dialog.
    pub fn show(&mut self, title: &str, prompt: &str, initial_value: &str) {
        self.title = title.to_string();
        self.prompt = prompt.to_string();
        self.input = initial_value.to_string();
        self.cursor = self.input.len();
        self.visible = true;
        self.focused = 0;
        self.hovered_button = None;
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
    pub fn handle_key(&mut self, key: &Key) -> Option<InputResult> {
        if !self.visible {
            return None;
        }

        match key {
            Key::Escape => {
                self.hide();
                Some(InputResult::Cancelled)
            }
            Key::Return => {
                if self.focused == 0 || self.focused == 1 {
                    let result = self.input.clone();
                    self.hide();
                    Some(InputResult::Submitted(result))
                } else {
                    self.hide();
                    Some(InputResult::Cancelled)
                }
            }
            Key::Tab => {
                self.focused = (self.focused + 1) % 3;
                None
            }
            Key::Char(c) if self.focused == 0 => {
                self.input.insert(self.cursor, *c);
                self.cursor += 1;
                None
            }
            Key::Backspace if self.focused == 0 => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.input.remove(self.cursor);
                }
                None
            }
            Key::Delete if self.focused == 0 => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
                None
            }
            Key::Left if self.focused == 0 => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                None
            }
            Key::Right if self.focused == 0 => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
                None
            }
            Key::Home if self.focused == 0 => {
                self.cursor = 0;
                None
            }
            Key::End if self.focused == 0 => {
                self.cursor = self.input.len();
                None
            }
            _ => None,
        }
    }

    /// Handle mouse click. Returns Some(result) if dialog should close.
    pub fn on_click(&mut self, pos: Point) -> Option<InputResult> {
        if !self.visible {
            return None;
        }

        let (ok_rect, cancel_rect) = self.button_rects();
        let input_rect = self.input_rect();

        if input_rect.contains_point(pos) {
            self.focused = 0;
            return None;
        }

        if ok_rect.contains_point(pos) {
            let result = self.input.clone();
            self.hide();
            return Some(InputResult::Submitted(result));
        }

        if cancel_rect.contains_point(pos) {
            self.hide();
            return Some(InputResult::Cancelled);
        }

        None
    }

    /// Handle mouse move.
    pub fn on_mouse_move(&mut self, pos: Point) {
        if !self.visible {
            return;
        }

        let (ok_rect, cancel_rect) = self.button_rects();

        if ok_rect.contains_point(pos) {
            self.hovered_button = Some(1);
        } else if cancel_rect.contains_point(pos) {
            self.hovered_button = Some(2);
        } else {
            self.hovered_button = None;
        }
    }

    /// Get the dialog rectangle.
    fn dialog_rect(&self) -> Rect {
        let dialog_width = 400.min(self.bounds.width.saturating_sub(40));
        let dialog_height = 160.min(self.bounds.height.saturating_sub(40));
        let x = self.bounds.x + (self.bounds.width as i32 - dialog_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - dialog_height as i32) / 2;
        Rect::new(x, y, dialog_width, dialog_height)
    }

    /// Get the input field rectangle.
    fn input_rect(&self) -> Rect {
        let dialog = self.dialog_rect();
        let input_width = dialog.width.saturating_sub(40);
        let input_height = 32;
        let x = dialog.x + 20;
        let y = dialog.y + 70;
        Rect::new(x, y, input_width, input_height)
    }

    /// Get button rectangles (ok, cancel).
    fn button_rects(&self) -> (Rect, Rect) {
        let dialog = self.dialog_rect();
        let button_width = 80;
        let button_height = 28;
        let button_y = dialog.y + dialog.height as i32 - button_height as i32 - 16;
        let button_gap = 16;
        let total_width = button_width * 2 + button_gap;
        let start_x = dialog.x + (dialog.width as i32 - total_width as i32) / 2;

        let ok_rect = Rect::new(start_x, button_y, button_width, button_height);
        let cancel_rect = Rect::new(start_x + button_width as i32 + button_gap as i32, button_y, button_width, button_height);

        (ok_rect, cancel_rect)
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

        // Prompt
        let prompt_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);

        renderer.text(
            &self.prompt,
            (dialog_rect.x + 20) as f64,
            (dialog_rect.y + 48) as f64,
            &prompt_style,
        )?;

        // Input field
        let input_rect = self.input_rect();
        let input_focused = self.focused == 0;

        renderer.fill_rounded_rect(input_rect, 4.0, theme.item_background)?;
        if input_focused {
            renderer.stroke_rounded_rect(input_rect, 4.0, theme.selection_background, 2.0)?;
        } else {
            renderer.stroke_rounded_rect(input_rect, 4.0, theme.border, 1.0)?;
        }

        // Input text
        let input_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        let text_y = input_rect.y + (input_rect.height as i32 - theme.font_size as i32) / 2;
        renderer.text(
            &self.input,
            (input_rect.x + 8) as f64,
            text_y as f64,
            &input_style,
        )?;

        // Cursor (if input is focused)
        if input_focused {
            let cursor_text = &self.input[..self.cursor];
            let cursor_x = if cursor_text.is_empty() {
                input_rect.x + 8
            } else {
                let width = renderer.measure_text(cursor_text, &input_style)
                    .map(|m| m.width as i32)
                    .unwrap_or(0);
                input_rect.x + 8 + width
            };

            renderer.line(
                cursor_x as f64,
                (input_rect.y + 6) as f64,
                cursor_x as f64,
                (input_rect.y + input_rect.height as i32 - 6) as f64,
                theme.foreground,
                1.0,
            )?;
        }

        // Buttons
        let (ok_rect, cancel_rect) = self.button_rects();

        let button_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        // OK button
        let ok_focused = self.focused == 1;
        let ok_hovered = self.hovered_button == Some(1);
        let ok_bg = if ok_focused || ok_hovered {
            theme.item_hover_background
        } else {
            theme.item_background
        };
        renderer.fill_rounded_rect(ok_rect, 4.0, ok_bg)?;
        if ok_focused {
            renderer.stroke_rounded_rect(ok_rect, 4.0, theme.foreground, 2.0)?;
        }

        let ok_text = "OK";
        let ok_width = renderer.measure_text(ok_text, &button_style)?.width;
        let ok_x = ok_rect.x + (ok_rect.width as i32 - ok_width as i32) / 2;
        let button_text_y = ok_rect.y + (ok_rect.height as i32 - theme.font_size as i32) / 2;
        renderer.text(ok_text, ok_x as f64, button_text_y as f64, &button_style)?;

        // Cancel button
        let cancel_focused = self.focused == 2;
        let cancel_hovered = self.hovered_button == Some(2);
        let cancel_bg = if cancel_focused || cancel_hovered {
            theme.item_hover_background
        } else {
            theme.item_background
        };
        renderer.fill_rounded_rect(cancel_rect, 4.0, cancel_bg)?;
        if cancel_focused {
            renderer.stroke_rounded_rect(cancel_rect, 4.0, theme.foreground, 2.0)?;
        }

        let cancel_text = "Cancel";
        let cancel_width = renderer.measure_text(cancel_text, &button_style)?.width;
        let cancel_x = cancel_rect.x + (cancel_rect.width as i32 - cancel_width as i32) / 2;
        renderer.text(cancel_text, cancel_x as f64, button_text_y as f64, &button_style)?;

        Ok(())
    }
}
