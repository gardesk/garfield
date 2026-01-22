//! List view component for displaying directory contents.

use crate::core::{EntryType, FileEntry};
use gartk_core::{Color, Rect};
use gartk_render::{Renderer, TextStyle};

/// Height of each row in the list view.
pub const ROW_HEIGHT: u32 = 28;

/// List view for displaying file entries.
pub struct ListView {
    /// Entries to display.
    entries: Vec<FileEntry>,
    /// Currently selected index.
    selected: usize,
    /// Scroll offset (first visible row).
    scroll_offset: usize,
    /// View bounds.
    bounds: Rect,
    /// Show hidden files.
    show_hidden: bool,
}

impl ListView {
    /// Create a new list view.
    pub fn new(bounds: Rect) -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            bounds,
            show_hidden: false,
        }
    }

    /// Set the entries to display.
    pub fn set_entries(&mut self, entries: Vec<FileEntry>) {
        self.entries = entries;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Get visible entries (respecting hidden filter).
    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        self.entries
            .iter()
            .filter(|e| self.show_hidden || !e.hidden)
            .collect()
    }

    /// Get the currently selected entry.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        let visible = self.visible_entries();
        visible.get(self.selected).copied()
    }

    /// Get the number of visible rows that fit in the view.
    fn visible_rows(&self) -> usize {
        (self.bounds.height / ROW_HEIGHT).max(1) as usize
    }

    /// Toggle hidden files visibility.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        // Clamp selection
        let visible_count = self.visible_entries().len();
        if self.selected >= visible_count && visible_count > 0 {
            self.selected = visible_count - 1;
        }
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            // Scroll if needed
            if self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        let visible_count = self.visible_entries().len();
        if self.selected + 1 < visible_count {
            self.selected += 1;
            // Scroll if needed
            let visible_rows = self.visible_rows();
            if self.selected >= self.scroll_offset + visible_rows {
                self.scroll_offset = self.selected - visible_rows + 1;
            }
        }
    }

    /// Jump to first entry.
    pub fn select_first(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Jump to last entry.
    pub fn select_last(&mut self) {
        let visible_count = self.visible_entries().len();
        if visible_count > 0 {
            self.selected = visible_count - 1;
            let visible_rows = self.visible_rows();
            if self.selected >= visible_rows {
                self.scroll_offset = self.selected - visible_rows + 1;
            }
        }
    }

    /// Page up.
    pub fn page_up(&mut self) {
        let page_size = self.visible_rows();
        if self.selected >= page_size {
            self.selected -= page_size;
        } else {
            self.selected = 0;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    /// Page down.
    pub fn page_down(&mut self) {
        let visible_count = self.visible_entries().len();
        let page_size = self.visible_rows();
        self.selected = (self.selected + page_size).min(visible_count.saturating_sub(1));
        if self.selected >= self.scroll_offset + page_size {
            self.scroll_offset = self.selected - page_size + 1;
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Render the list view.
    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let visible = self.visible_entries();
        let visible_rows = self.visible_rows();

        // Calculate column widths
        let name_width = (self.bounds.width as f64 * 0.5) as u32;
        let size_width = 100;
        let date_width = self.bounds.width.saturating_sub(name_width + size_width + 32);

        // Draw header
        let header_rect = Rect::new(
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            ROW_HEIGHT,
        );
        renderer.fill_rect(header_rect, theme.item_background.darken(0.1))?;

        let header_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground.with_alpha(0.7));

        // Header labels
        let name_rect = Rect::new(header_rect.x + 8, header_rect.y, name_width, ROW_HEIGHT);
        renderer.text_in_rect("Name", name_rect, &header_style)?;

        let size_rect = Rect::new(
            header_rect.x + name_width as i32 + 16,
            header_rect.y,
            size_width,
            ROW_HEIGHT,
        );
        renderer.text_in_rect("Size", size_rect, &header_style)?;

        let date_rect = Rect::new(
            header_rect.x + name_width as i32 + size_width as i32 + 24,
            header_rect.y,
            date_width,
            ROW_HEIGHT,
        );
        renderer.text_in_rect("Modified", date_rect, &header_style)?;

        // Draw entries
        for (i, entry) in visible
            .iter()
            .skip(self.scroll_offset)
            .take(visible_rows)
            .enumerate()
        {
            let y = self.bounds.y + ((i + 1) as i32 * ROW_HEIGHT as i32);
            let row_rect = Rect::new(self.bounds.x, y, self.bounds.width, ROW_HEIGHT);

            let actual_index = self.scroll_offset + i;
            let is_selected = actual_index == self.selected;

            // Row background
            if is_selected {
                renderer.fill_rounded_rect(row_rect, 4.0, theme.item_selected_background)?;
            } else if i % 2 == 1 {
                renderer.fill_rect(row_rect, theme.item_background.with_alpha(0.3))?;
            }

            // Determine colors
            let text_color = if is_selected {
                theme.selection_foreground
            } else if entry.hidden {
                theme.item_foreground.with_alpha(0.5)
            } else {
                theme.item_foreground
            };

            let name_color = match entry.entry_type {
                EntryType::Directory => Color::from_hex("#5c9fd8").unwrap_or(text_color),
                EntryType::Symlink => Color::from_hex("#c678dd").unwrap_or(text_color),
                _ => text_color,
            };

            let text_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size)
                .color(text_color);

            let name_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size)
                .color(if is_selected { theme.selection_foreground } else { name_color });

            // Name with icon prefix
            let icon = match entry.entry_type {
                EntryType::Directory => "\u{1F4C1} ", // folder emoji
                EntryType::Symlink => "\u{1F517} ",   // link emoji
                _ => "\u{1F4C4} ",                     // file emoji
            };
            let display_name = format!("{}{}", icon, entry.name);

            let name_rect = Rect::new(row_rect.x + 8, row_rect.y, name_width, ROW_HEIGHT);
            renderer.text_in_rect(&display_name, name_rect, &name_style)?;

            // Size
            let size_rect = Rect::new(
                row_rect.x + name_width as i32 + 16,
                row_rect.y,
                size_width,
                ROW_HEIGHT,
            );
            renderer.text_in_rect(&entry.format_size(), size_rect, &text_style)?;

            // Modified date
            let date_rect = Rect::new(
                row_rect.x + name_width as i32 + size_width as i32 + 24,
                row_rect.y,
                date_width,
                ROW_HEIGHT,
            );
            renderer.text_in_rect(&entry.format_modified(), date_rect, &text_style)?;
        }

        Ok(())
    }
}
