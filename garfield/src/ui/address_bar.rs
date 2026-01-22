//! Address bar for direct path entry.

use gartk_core::{Key, Rect};
use gartk_render::{Renderer, TextStyle};
use std::path::PathBuf;

/// Address bar for editing the current path.
pub struct AddressBar {
    /// Current text being edited.
    text: String,
    /// Cursor position in the text.
    cursor: usize,
    /// Component bounds.
    bounds: Rect,
    /// Whether the address bar is active (visible and editable).
    active: bool,
    /// Padding.
    padding: u32,
    /// Blink counter for cursor animation.
    blink_counter: u32,
}

/// Frames per blink cycle (on + off).
const BLINK_RATE: u32 = 30;

impl AddressBar {
    /// Create a new address bar.
    pub fn new(bounds: Rect) -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            bounds,
            active: false,
            padding: 8,
            blink_counter: 0,
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Check if address bar is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate the address bar with the given path.
    pub fn activate(&mut self, path: &PathBuf) {
        self.text = path.to_string_lossy().to_string();
        self.cursor = self.text.len();
        self.active = true;
        self.blink_counter = 0;
    }

    /// Deactivate the address bar without navigating.
    pub fn cancel(&mut self) {
        self.active = false;
        self.text.clear();
        self.cursor = 0;
    }

    /// Confirm the current text and return the path to navigate to (if valid).
    pub fn confirm(&mut self) -> Option<PathBuf> {
        if !self.active {
            return None;
        }

        self.active = false;
        let path = PathBuf::from(&self.text);
        self.text.clear();
        self.cursor = 0;

        // Expand ~ to home directory
        let expanded = if path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                if path == PathBuf::from("~") {
                    home
                } else {
                    home.join(path.strip_prefix("~").unwrap_or(&path))
                }
            } else {
                path
            }
        } else {
            path
        };

        if expanded.is_dir() {
            Some(expanded)
        } else {
            // Try parent if it's a file path
            expanded.parent().map(|p| p.to_path_buf()).filter(|p| p.is_dir())
        }
    }

    /// Handle a key press. Returns true if the key was consumed.
    pub fn handle_key(&mut self, key: &Key) -> bool {
        if !self.active {
            return false;
        }

        // Reset blink on keypress so cursor stays visible while typing
        self.blink_counter = 0;

        match key {
            Key::Escape => {
                self.cancel();
                true
            }
            Key::Return => {
                // Handled by caller via confirm()
                true
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.text.remove(self.cursor);
                }
                true
            }
            Key::Delete => {
                if self.cursor < self.text.len() {
                    self.text.remove(self.cursor);
                }
                true
            }
            Key::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                true
            }
            Key::Right => {
                if self.cursor < self.text.len() {
                    self.cursor += 1;
                }
                true
            }
            Key::Home => {
                self.cursor = 0;
                true
            }
            Key::End => {
                self.cursor = self.text.len();
                true
            }
            Key::Char(c) => {
                self.text.insert(self.cursor, *c);
                self.cursor += 1;
                true
            }
            _ => false,
        }
    }

    /// Render the address bar.
    pub fn render(&mut self, renderer: &Renderer) -> anyhow::Result<()> {
        if !self.active {
            return Ok(());
        }

        let theme = renderer.theme();

        // Draw distinct background for edit mode
        let edit_bg = theme.input_background;
        renderer.fill_rect(self.bounds, edit_bg)?;

        // Draw prominent border to indicate edit mode
        renderer.stroke_rect(self.bounds, theme.selection_background, 2.0)?;

        // Text style
        let style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 1.0)
            .color(theme.input_foreground);

        // Draw text
        let text_x = self.bounds.x + self.padding as i32;
        let text_y = self.bounds.y + (self.bounds.height as i32 - theme.font_size as i32) / 2;
        renderer.text(&self.text, text_x as f64, text_y as f64, &style)?;

        // Blinking cursor - visible for first half of blink cycle
        self.blink_counter = (self.blink_counter + 1) % BLINK_RATE;
        let cursor_visible = self.blink_counter < BLINK_RATE / 2;

        if cursor_visible {
            let text_before_cursor = &self.text[..self.cursor];
            let cursor_offset = renderer.measure_text(text_before_cursor, &style)?.width;
            let cursor_x = text_x + cursor_offset as i32;
            let cursor_y = self.bounds.y + 6;
            let cursor_height = self.bounds.height - 12;

            renderer.fill_rect(
                gartk_core::Rect::new(cursor_x, cursor_y, 2, cursor_height),
                theme.input_cursor,
            )?;
        }

        Ok(())
    }
}
