//! Help modal showing keyboard shortcuts.

use anyhow::Result;
use gartk_core::{Point, Rect, Theme};
use gartk_render::{Renderer, TextStyle};

/// Help modal overlay.
pub struct HelpModal {
    bounds: Rect,
    visible: bool,
    scroll_offset: i32,
    content_height: i32,
}

/// A keybind entry for display.
struct KeybindEntry {
    key: &'static str,
    description: &'static str,
}

const KEYBINDS: &[(&str, &[KeybindEntry])] = &[
    ("Navigation", &[
        KeybindEntry { key: "Up/Down", description: "Move selection" },
        KeybindEntry { key: "Left/Right", description: "Grid navigation" },
        KeybindEntry { key: "Enter", description: "Open selected" },
        KeybindEntry { key: "Backspace", description: "Go to parent" },
        KeybindEntry { key: "Alt+Left", description: "Go back" },
        KeybindEntry { key: "Alt+Right", description: "Go forward" },
        KeybindEntry { key: "Home", description: "Select first" },
        KeybindEntry { key: "End", description: "Select last" },
        KeybindEntry { key: "PgUp/PgDn", description: "Page up/down" },
    ]),
    ("Views", &[
        KeybindEntry { key: "Ctrl+1", description: "List view" },
        KeybindEntry { key: "Ctrl+2", description: "Grid view" },
        KeybindEntry { key: "Ctrl+3", description: "Column view" },
        KeybindEntry { key: "Ctrl+H", description: "Toggle hidden files" },
        KeybindEntry { key: "Ctrl+=", description: "Cycle icon size" },
    ]),
    ("Tabs & Panes", &[
        KeybindEntry { key: "Ctrl+T", description: "New tab" },
        KeybindEntry { key: "Ctrl+W", description: "Close tab" },
        KeybindEntry { key: "Ctrl+Tab", description: "Next tab" },
        KeybindEntry { key: "Ctrl+Shift+Tab", description: "Previous tab" },
        KeybindEntry { key: "Alt+1-9", description: "Switch to tab N" },
        KeybindEntry { key: "Middle-click", description: "Close tab" },
        KeybindEntry { key: "Ctrl+Shift+H", description: "Split horizontal" },
        KeybindEntry { key: "Ctrl+Shift+V", description: "Split vertical" },
        KeybindEntry { key: "Ctrl+Shift+W", description: "Close pane" },
        KeybindEntry { key: "Ctrl+Shift+Arrow", description: "Focus pane" },
    ]),
    ("Selection", &[
        KeybindEntry { key: "Ctrl+A", description: "Select all" },
        KeybindEntry { key: "Ctrl+Click", description: "Toggle selection" },
        KeybindEntry { key: "Shift+Click", description: "Range select" },
        KeybindEntry { key: "Shift+Up/Down", description: "Extend selection" },
    ]),
    ("File Operations", &[
        KeybindEntry { key: "Ctrl+C", description: "Copy" },
        KeybindEntry { key: "Ctrl+X", description: "Cut" },
        KeybindEntry { key: "Ctrl+V", description: "Paste" },
        KeybindEntry { key: "Ctrl+Z", description: "Undo" },
        KeybindEntry { key: "Ctrl+Y", description: "Redo" },
        KeybindEntry { key: "Delete", description: "Move to trash" },
        KeybindEntry { key: "Shift+Delete", description: "Delete permanently" },
        KeybindEntry { key: "F2", description: "Rename" },
        KeybindEntry { key: "Ctrl+Shift+N", description: "New folder" },
    ]),
    ("Bookmarks", &[
        KeybindEntry { key: "Ctrl+D", description: "Add bookmark" },
        KeybindEntry { key: "Drag folder", description: "Drop on sidebar" },
    ]),
    ("Other", &[
        KeybindEntry { key: "Ctrl+L", description: "Edit address" },
        KeybindEntry { key: "F5", description: "Refresh" },
        KeybindEntry { key: "F1", description: "Toggle help" },
        KeybindEntry { key: "Escape", description: "Close modal/quit" },
        KeybindEntry { key: "q", description: "Quit" },
    ]),
];

impl HelpModal {
    /// Create a new help modal.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            visible: false,
            scroll_offset: 0,
            content_height: Self::calculate_content_height(),
        }
    }

    /// Calculate total content height based on keybind entries.
    fn calculate_content_height() -> i32 {
        let line_height = 21; // Approximate: font_size * 1.5
        let section_gap = 8;
        let section_header_extra = 4;

        let mut height = 0;
        for (_, entries) in KEYBINDS {
            height += line_height + section_header_extra; // Section header
            height += entries.len() as i32 * line_height; // Entries
            height += section_gap;
        }
        height
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Show the modal.
    pub fn show(&mut self) {
        self.visible = true;
        self.scroll_offset = 0;
    }

    /// Hide the modal.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Handle click. Returns true if click was inside modal.
    pub fn on_click(&mut self, pos: Point) -> bool {
        if !self.visible {
            return false;
        }

        let modal_rect = self.modal_rect();
        if !modal_rect.contains_point(pos) {
            self.hide();
            return true;
        }

        true
    }

    /// Handle mouse wheel scroll.
    pub fn on_scroll(&mut self, delta: i32) {
        if !self.visible {
            return;
        }
        if delta > 0 {
            self.scroll_up();
        } else if delta < 0 {
            self.scroll_down();
        }
    }

    /// Scroll up.
    pub fn scroll_up(&mut self) {
        self.scroll_offset = (self.scroll_offset - 20).max(0);
    }

    /// Scroll down.
    pub fn scroll_down(&mut self) {
        let modal_rect = self.modal_rect();
        let visible_height = modal_rect.height as i32 - 60; // Account for title
        let max_scroll = (self.content_height - visible_height).max(0);
        self.scroll_offset = (self.scroll_offset + 20).min(max_scroll);
    }

    /// Get the modal rectangle (centered in bounds).
    fn modal_rect(&self) -> Rect {
        let modal_width = 400.min(self.bounds.width.saturating_sub(40));
        let modal_height = 500.min(self.bounds.height.saturating_sub(40));
        let x = self.bounds.x + (self.bounds.width as i32 - modal_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - modal_height as i32) / 2;
        Rect::new(x, y, modal_width, modal_height)
    }

    /// Render the modal.
    pub fn render(&self, renderer: &Renderer) -> Result<()> {
        if !self.visible {
            return Ok(());
        }

        let theme = renderer.theme();

        // Dim background overlay
        renderer.fill_rect(self.bounds, gartk_core::Color::from_u8(0, 0, 0, 128))?;

        let modal_rect = self.modal_rect();

        // Modal background
        renderer.fill_rounded_rect(modal_rect, 8.0, theme.background)?;
        renderer.stroke_rounded_rect(modal_rect, 8.0, theme.border, 1.0)?;

        // Title
        let title_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 4.0)
            .color(theme.foreground);

        let title = "Keyboard Shortcuts";
        renderer.text(
            title,
            (modal_rect.x + 20) as f64,
            (modal_rect.y + 20) as f64,
            &title_style,
        )?;

        // Content area with clipping
        let content_rect = Rect::new(
            modal_rect.x + 10,
            modal_rect.y + 50,
            modal_rect.width - 20,
            modal_rect.height - 60,
        );

        self.render_keybinds(renderer, content_rect, theme)?;

        // Render scroll indicator if content overflows
        let visible_height = content_rect.height as i32;
        if self.content_height > visible_height {
            self.render_scrollbar(renderer, modal_rect, visible_height, theme)?;
        }

        Ok(())
    }

    /// Render a scrollbar indicator.
    fn render_scrollbar(&self, renderer: &Renderer, modal_rect: Rect, visible_height: i32, theme: &Theme) -> Result<()> {
        let scrollbar_width = 4;
        let scrollbar_x = modal_rect.x + modal_rect.width as i32 - scrollbar_width - 8;
        let scrollbar_y = modal_rect.y + 50;
        let scrollbar_height = visible_height as u32;

        // Track background
        let track_rect = Rect::new(scrollbar_x, scrollbar_y, scrollbar_width as u32, scrollbar_height);
        renderer.fill_rounded_rect(track_rect, 2.0, theme.item_background)?;

        // Thumb
        let thumb_ratio = visible_height as f64 / self.content_height as f64;
        let thumb_height = ((scrollbar_height as f64 * thumb_ratio) as u32).max(20);
        let max_scroll = (self.content_height - visible_height).max(1);
        let scroll_ratio = self.scroll_offset as f64 / max_scroll as f64;
        let thumb_y = scrollbar_y + ((scrollbar_height - thumb_height) as f64 * scroll_ratio) as i32;

        let thumb_rect = Rect::new(scrollbar_x, thumb_y, scrollbar_width as u32, thumb_height);
        renderer.fill_rounded_rect(thumb_rect, 2.0, theme.item_foreground)?;

        Ok(())
    }

    /// Render keybind sections.
    fn render_keybinds(&self, renderer: &Renderer, rect: Rect, theme: &Theme) -> Result<()> {
        let section_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 1.0)
            .color(theme.selection_background);

        let key_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        let desc_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_description);

        let line_height = (theme.font_size * 1.5) as i32;
        let section_gap = 8;
        let key_col_width = 140;

        let mut y = rect.y - self.scroll_offset;

        for (section_name, entries) in KEYBINDS {
            // Section header
            if y >= rect.y && y < rect.y + rect.height as i32 {
                renderer.text(
                    section_name,
                    (rect.x + 10) as f64,
                    y as f64,
                    &section_style,
                )?;
            }
            y += line_height + 4;

            // Entries
            for entry in *entries {
                if y >= rect.y && y < rect.y + rect.height as i32 {
                    renderer.text(
                        entry.key,
                        (rect.x + 20) as f64,
                        y as f64,
                        &key_style,
                    )?;
                    renderer.text(
                        entry.description,
                        (rect.x + 20 + key_col_width) as f64,
                        y as f64,
                        &desc_style,
                    )?;
                }
                y += line_height;
            }

            y += section_gap;
        }

        Ok(())
    }
}
