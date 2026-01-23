//! Help modal showing keyboard shortcuts.

use anyhow::Result;
use gartk_core::{Point, Rect, Theme};
use gartk_render::{Renderer, TextStyle};

/// Help modal overlay.
pub struct HelpModal {
    bounds: Rect,
    visible: bool,
    scroll_offset: i32,
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
    ("Other", &[
        KeybindEntry { key: "Ctrl+L", description: "Edit address" },
        KeybindEntry { key: "F5", description: "Refresh" },
        KeybindEntry { key: "F1", description: "Toggle help" },
        KeybindEntry { key: "Escape", description: "Close modal" },
    ]),
];

impl HelpModal {
    /// Create a new help modal.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            visible: false,
            scroll_offset: 0,
        }
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

    /// Scroll up.
    pub fn scroll_up(&mut self) {
        self.scroll_offset = (self.scroll_offset - 20).max(0);
    }

    /// Scroll down.
    pub fn scroll_down(&mut self) {
        self.scroll_offset += 20;
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
