//! Status bar showing selection count and directory info.

use gartk_core::Rect;
use gartk_render::{Renderer, TextStyle};

/// Height of the status bar.
pub const STATUS_BAR_HEIGHT: u32 = 24;

/// Status bar component.
pub struct StatusBar {
    /// Component bounds.
    bounds: Rect,
    /// Total number of items in directory.
    total_items: usize,
    /// Number of selected items.
    selected_count: usize,
    /// Total size of selected items.
    selected_size: u64,
    /// Current view mode name.
    view_mode: String,
}

impl StatusBar {
    /// Create a new status bar.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            total_items: 0,
            selected_count: 0,
            selected_size: 0,
            view_mode: "List".to_string(),
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Update the status with current directory info.
    pub fn update(&mut self, total_items: usize, selected_count: usize, selected_size: u64) {
        self.total_items = total_items;
        self.selected_count = selected_count;
        self.selected_size = selected_size;
    }

    /// Set the current view mode name.
    pub fn set_view_mode(&mut self, mode: &str) {
        self.view_mode = mode.to_string();
    }

    /// Get status bar height.
    pub fn height(&self) -> u32 {
        STATUS_BAR_HEIGHT
    }

    /// Render the status bar.
    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        let theme = renderer.theme();

        // Draw background
        renderer.fill_rect(self.bounds, theme.item_background.darken(0.05))?;

        // Draw top border
        renderer.line(
            self.bounds.x as f64,
            self.bounds.y as f64,
            (self.bounds.x + self.bounds.width as i32) as f64,
            self.bounds.y as f64,
            theme.border,
            1.0,
        )?;

        let text_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.item_foreground.with_alpha(0.8));

        let padding = 12;
        let text_y = self.bounds.y + (self.bounds.height as i32 - theme.font_size as i32) / 2;

        // Left side: item count and selection
        let left_text = if self.selected_count > 1 {
            format!(
                "{} items ({} selected, {})",
                self.total_items,
                self.selected_count,
                format_bytes(self.selected_size)
            )
        } else if self.selected_count == 1 {
            format!(
                "{} items (1 selected, {})",
                self.total_items,
                format_bytes(self.selected_size)
            )
        } else {
            format!("{} items", self.total_items)
        };

        renderer.text(
            &left_text,
            (self.bounds.x + padding) as f64,
            text_y as f64,
            &text_style,
        )?;

        // Right side: view mode
        let mode_width = renderer.measure_text(&self.view_mode, &text_style)?.width;
        let mode_x = self.bounds.x + self.bounds.width as i32 - mode_width as i32 - padding;
        renderer.text(&self.view_mode, mode_x as f64, text_y as f64, &text_style)?;

        Ok(())
    }
}

/// Format bytes as human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
