//! Status bar showing selection count and directory info.

use gartk_core::Rect;
use gartk_render::{Renderer, TextStyle};
use std::ffi::CString;
use std::path::Path;

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
    /// Free disk space in bytes.
    free_space: Option<u64>,
    /// Status message for operations (e.g., "3 files copied").
    status_message: Option<String>,
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
            free_space: None,
            status_message: None,
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

    /// Light-weight update of just the selection count (for drag operations).
    pub fn update_selection_count(&mut self, selected_count: usize) {
        self.selected_count = selected_count;
    }

    /// Set the current view mode name.
    pub fn set_view_mode(&mut self, mode: &str) {
        self.view_mode = mode.to_string();
    }

    /// Update free disk space for the given path.
    pub fn update_free_space(&mut self, path: &Path) {
        self.free_space = get_free_space(path);
    }

    /// Set a status message to display (replaces left side text temporarily).
    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// Clear the status message.
    pub fn clear_status_message(&mut self) {
        self.status_message = None;
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

        // Left side: status message (if any) or item count and selection
        let left_text = if let Some(ref msg) = self.status_message {
            msg.clone()
        } else if self.selected_count > 1 {
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

        // Use highlight color for status messages
        let left_style = if self.status_message.is_some() {
            text_style.clone().color(theme.selection_background)
        } else {
            text_style.clone()
        };

        renderer.text(
            &left_text,
            (self.bounds.x + padding) as f64,
            text_y as f64,
            &left_style,
        )?;

        // Right side: free space and view mode
        let mut right_x = self.bounds.x + self.bounds.width as i32 - padding;

        // View mode
        let mode_width = renderer.measure_text(&self.view_mode, &text_style)?.width;
        right_x -= mode_width as i32;
        renderer.text(&self.view_mode, right_x as f64, text_y as f64, &text_style)?;

        // Free space (if available)
        if let Some(free) = self.free_space {
            let free_text = format!("{} free  |  ", format_bytes(free));
            let free_width = renderer.measure_text(&free_text, &text_style)?.width;
            right_x -= free_width as i32;
            renderer.text(&free_text, right_x as f64, text_y as f64, &text_style)?;
        }

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

/// Get free disk space for the filesystem containing the given path.
fn get_free_space(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
            // Available blocks * block size = free space for non-privileged users
            Some(stat.f_bavail as u64 * stat.f_bsize as u64)
        } else {
            None
        }
    }
}
