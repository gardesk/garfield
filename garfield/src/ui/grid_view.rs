//! Grid/icon view for displaying directory contents.

use crate::core::{EntryType, FileEntry, SortDirection, SortOrder};
use gartk_core::{Color, Modifiers, Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::collections::HashSet;

/// Size of each grid cell.
pub const CELL_SIZE: u32 = 100;

/// Icon size within each cell.
pub const ICON_SIZE: u32 = 48;

/// Padding around cells.
pub const CELL_PADDING: u32 = 8;

/// Grid view for displaying file entries as icons.
pub struct GridView {
    /// Entries to display.
    entries: Vec<FileEntry>,
    /// Currently focused index (for keyboard nav).
    focused: usize,
    /// Selected indices (for multi-select).
    selected: HashSet<usize>,
    /// Anchor index for shift-selection.
    selection_anchor: Option<usize>,
    /// Scroll offset (first visible row index).
    scroll_offset: usize,
    /// View bounds.
    bounds: Rect,
    /// Show hidden files.
    show_hidden: bool,
    /// Columns in the grid.
    columns: usize,
    /// Hovered item index.
    hovered: Option<usize>,
    /// Rubber band drag start position.
    drag_start: Option<Point>,
    /// Rubber band current position.
    drag_current: Option<Point>,
}

impl GridView {
    /// Create a new grid view.
    pub fn new(bounds: Rect) -> Self {
        let columns = Self::calculate_columns(bounds.width);
        Self {
            entries: Vec::new(),
            focused: 0,
            selected: HashSet::new(),
            selection_anchor: None,
            scroll_offset: 0,
            bounds,
            show_hidden: false,
            columns,
            hovered: None,
            drag_start: None,
            drag_current: None,
        }
    }

    /// Calculate number of columns that fit in the given width.
    fn calculate_columns(width: u32) -> usize {
        ((width - CELL_PADDING) / (CELL_SIZE + CELL_PADDING)).max(1) as usize
    }

    /// Set the entries to display.
    pub fn set_entries(&mut self, entries: Vec<FileEntry>) {
        self.entries = entries;
        self.focused = 0;
        self.selected.clear();
        self.selected.insert(0);
        self.selection_anchor = Some(0);
        self.scroll_offset = 0;
    }

    /// Get visible entries (respecting hidden filter).
    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        self.entries
            .iter()
            .filter(|e| self.show_hidden || !e.hidden)
            .collect()
    }

    /// Get the currently focused entry.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        let visible = self.visible_entries();
        visible.get(self.focused).copied()
    }

    /// Get all selected entries.
    pub fn selected_entries(&self) -> Vec<&FileEntry> {
        let visible = self.visible_entries();
        self.selected
            .iter()
            .filter_map(|&i| visible.get(i).copied())
            .collect()
    }

    /// Get selection count.
    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    /// Get the number of visible rows that fit in the view.
    fn visible_rows(&self) -> usize {
        (self.bounds.height / (CELL_SIZE + CELL_PADDING)).max(1) as usize
    }

    /// Get current sort settings (grid view doesn't track these, uses external).
    pub fn sort_settings(&self) -> (SortOrder, SortDirection) {
        (SortOrder::Name, SortDirection::Ascending)
    }

    /// Toggle hidden files visibility.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let visible_count = self.visible_entries().len();
        if self.focused >= visible_count && visible_count > 0 {
            self.focused = visible_count - 1;
        }
        self.selected.retain(|&i| i < visible_count);
        if self.selected.is_empty() && visible_count > 0 {
            self.selected.insert(self.focused);
        }
    }

    /// Move selection up (to previous row).
    pub fn select_prev(&mut self) {
        if self.focused >= self.columns {
            self.focused -= self.columns;
        } else if self.focused > 0 {
            self.focused = 0;
        }
        self.update_single_selection();
    }

    /// Move selection down (to next row).
    pub fn select_next(&mut self) {
        let visible_count = self.visible_entries().len();
        if self.focused + self.columns < visible_count {
            self.focused += self.columns;
        } else if self.focused < visible_count.saturating_sub(1) {
            self.focused = visible_count - 1;
        }
        self.update_single_selection();
    }

    /// Move selection left.
    pub fn select_left(&mut self) {
        if self.focused > 0 {
            self.focused -= 1;
            self.update_single_selection();
        }
    }

    /// Move selection right.
    pub fn select_right(&mut self) {
        let visible_count = self.visible_entries().len();
        if self.focused + 1 < visible_count {
            self.focused += 1;
            self.update_single_selection();
        }
    }

    /// Update selection to just the focused item and ensure visibility.
    fn update_single_selection(&mut self) {
        self.selected.clear();
        self.selected.insert(self.focused);
        self.selection_anchor = Some(self.focused);
        self.ensure_visible();
    }

    /// Ensure focused item is visible.
    fn ensure_visible(&mut self) {
        let row = self.focused / self.columns;
        let visible_rows = self.visible_rows();

        if row < self.scroll_offset {
            self.scroll_offset = row;
        } else if row >= self.scroll_offset + visible_rows {
            self.scroll_offset = row - visible_rows + 1;
        }
    }

    /// Jump to first entry.
    pub fn select_first(&mut self) {
        self.focused = 0;
        self.update_single_selection();
    }

    /// Jump to last entry.
    pub fn select_last(&mut self) {
        let visible_count = self.visible_entries().len();
        if visible_count > 0 {
            self.focused = visible_count - 1;
            self.update_single_selection();
        }
    }

    /// Page up.
    pub fn page_up(&mut self) {
        let page_size = self.visible_rows() * self.columns;
        if self.focused >= page_size {
            self.focused -= page_size;
        } else {
            self.focused = 0;
        }
        self.update_single_selection();
    }

    /// Page down.
    pub fn page_down(&mut self) {
        let visible_count = self.visible_entries().len();
        let page_size = self.visible_rows() * self.columns;
        self.focused = (self.focused + page_size).min(visible_count.saturating_sub(1));
        self.update_single_selection();
    }

    /// Select all entries (Ctrl+A).
    pub fn select_all(&mut self) {
        let visible_count = self.visible_entries().len();
        self.selected = (0..visible_count).collect();
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.columns = Self::calculate_columns(bounds.width);
    }

    /// Get cell bounds for an index.
    fn cell_bounds(&self, index: usize) -> Rect {
        let visible_index = index.saturating_sub(self.scroll_offset * self.columns);
        let col = visible_index % self.columns;
        let row = visible_index / self.columns;

        let x = self.bounds.x + CELL_PADDING as i32 + (col as i32 * (CELL_SIZE + CELL_PADDING) as i32);
        let y = self.bounds.y + CELL_PADDING as i32 + (row as i32 * (CELL_SIZE + CELL_PADDING) as i32);

        Rect::new(x, y, CELL_SIZE, CELL_SIZE)
    }

    /// Handle mouse move for hover effects and rubber band drag.
    pub fn on_mouse_move(&mut self, pos: Point) {
        // Handle rubber band drag
        if self.drag_start.is_some() {
            self.drag_current = Some(pos);
            self.update_rubber_band_selection();
            return;
        }

        if !self.bounds.contains_point(pos) {
            self.hovered = None;
            return;
        }

        let visible = self.visible_entries();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + self.visible_rows() * self.columns).min(visible.len());

        self.hovered = None;
        for i in start_index..end_index {
            if self.cell_bounds(i).contains_point(pos) {
                self.hovered = Some(i);
                break;
            }
        }
    }

    /// Start rubber band selection.
    pub fn start_drag(&mut self, pos: Point) {
        self.drag_start = Some(pos);
        self.drag_current = Some(pos);
        self.selected.clear();
    }

    /// Check if rubber band drag is active.
    pub fn is_dragging(&self) -> bool {
        self.drag_start.is_some()
    }

    /// Stop rubber band selection.
    pub fn stop_drag(&mut self) {
        self.drag_start = None;
        self.drag_current = None;
    }

    /// Get the rubber band rectangle (if dragging).
    pub fn rubber_band_rect(&self) -> Option<Rect> {
        if let (Some(start), Some(current)) = (self.drag_start, self.drag_current) {
            let x = start.x.min(current.x);
            let y = start.y.min(current.y);
            let w = (start.x - current.x).unsigned_abs();
            let h = (start.y - current.y).unsigned_abs();
            Some(Rect::new(x, y, w, h))
        } else {
            None
        }
    }

    /// Update selection based on rubber band rectangle.
    fn update_rubber_band_selection(&mut self) {
        if let Some(band) = self.rubber_band_rect() {
            let visible = self.visible_entries();
            let start_index = self.scroll_offset * self.columns;
            let end_index = (start_index + self.visible_rows() * self.columns).min(visible.len());

            self.selected.clear();
            for i in start_index..end_index {
                let cell = self.cell_bounds(i);
                if rects_intersect(&band, &cell) {
                    self.selected.insert(i);
                }
            }
        }
    }

    /// Handle cell click. Returns index of clicked cell if valid.
    pub fn on_click(&mut self, pos: Point, modifiers: &Modifiers) -> Option<usize> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        let visible = self.visible_entries();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + self.visible_rows() * self.columns).min(visible.len());

        for i in start_index..end_index {
            if self.cell_bounds(i).contains_point(pos) {
                if modifiers.ctrl {
                    if self.selected.contains(&i) {
                        self.selected.remove(&i);
                    } else {
                        self.selected.insert(i);
                    }
                    self.focused = i;
                    self.selection_anchor = Some(i);
                } else if modifiers.shift {
                    if let Some(anchor) = self.selection_anchor {
                        let (start, end) = if anchor <= i {
                            (anchor, i)
                        } else {
                            (i, anchor)
                        };
                        self.selected = (start..=end).collect();
                    } else {
                        self.selected.clear();
                        self.selected.insert(i);
                        self.selection_anchor = Some(i);
                    }
                    self.focused = i;
                } else {
                    self.selected.clear();
                    self.selected.insert(i);
                    self.focused = i;
                    self.selection_anchor = Some(i);
                }
                return Some(i);
            }
        }

        // Clicked in empty space - start rubber band
        if !modifiers.ctrl && !modifiers.shift {
            self.start_drag(pos);
        }

        None
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered = None;
        // Also stop any active drag
        self.stop_drag();
    }

    /// Render the grid view.
    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let visible = self.visible_entries();
        let visible_rows = self.visible_rows();

        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + visible_rows * self.columns).min(visible.len());

        for i in start_index..end_index {
            let entry = visible[i];
            let cell = self.cell_bounds(i);

            // Skip cells outside visible area
            if cell.y + cell.height as i32 <= self.bounds.y {
                continue;
            }
            if cell.y >= self.bounds.y + self.bounds.height as i32 {
                break;
            }

            let is_selected = self.selected.contains(&i);
            let is_focused = i == self.focused;
            let is_hovered = self.hovered == Some(i);

            // Cell background
            if is_selected {
                renderer.fill_rounded_rect(cell, 8.0, theme.item_selected_background)?;
            } else if is_hovered {
                renderer.fill_rounded_rect(cell, 8.0, theme.item_background)?;
            }

            // Focus indicator
            if is_focused && self.selected.len() > 1 {
                renderer.stroke_rect(cell, theme.selection_background.with_alpha(0.5), 1.0)?;
            }

            // Icon (placeholder using text)
            let icon = match entry.entry_type {
                EntryType::Directory => "\u{1F4C1}",  // folder emoji
                EntryType::Symlink => "\u{1F517}",    // link emoji
                _ => Self::file_icon_for_extension(entry.extension().as_deref()),
            };

            let icon_color = if is_selected {
                theme.selection_foreground
            } else {
                match entry.entry_type {
                    EntryType::Directory => Color::from_hex("#5c9fd8").unwrap_or(theme.item_foreground),
                    EntryType::Symlink => Color::from_hex("#c678dd").unwrap_or(theme.item_foreground),
                    _ => theme.item_foreground,
                }
            };

            let icon_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(32.0)
                .color(icon_color);

            let icon_x = cell.x + (cell.width as i32 - 32) / 2;
            let icon_y = cell.y + 10;
            renderer.text(icon, icon_x as f64, icon_y as f64, &icon_style)?;

            // File name (truncated)
            let name_color = if is_selected {
                theme.selection_foreground
            } else if entry.hidden {
                theme.item_foreground.with_alpha(0.5)
            } else {
                theme.item_foreground
            };

            let name_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size - 1.0)
                .color(name_color);

            // Truncate name if too long
            let max_chars = (cell.width / 7) as usize;
            let display_name = if entry.name.len() > max_chars {
                format!("{}...", &entry.name[..max_chars.saturating_sub(3)])
            } else {
                entry.name.clone()
            };

            // Center text horizontally below icon
            let center_x = cell.x + cell.width as i32 / 2;
            let center_y = cell.y + ICON_SIZE as i32 + 16;
            renderer.text_centered(&display_name, Point::new(center_x, center_y), &name_style)?;
        }

        // Draw rubber band selection rectangle
        if let Some(band) = self.rubber_band_rect() {
            let selection_color = theme.selection_background.with_alpha(0.3);
            let border_color = theme.selection_background;
            renderer.fill_rect(band, selection_color)?;
            renderer.stroke_rect(band, border_color, 1.0)?;
        }

        Ok(())
    }

    /// Get placeholder icon for file extension.
    fn file_icon_for_extension(ext: Option<&str>) -> &'static str {
        match ext {
            Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp") | Some("svg") => "\u{1F5BC}", // image
            Some("mp3") | Some("wav") | Some("flac") | Some("ogg") | Some("m4a") => "\u{1F3B5}", // music
            Some("mp4") | Some("mkv") | Some("avi") | Some("mov") | Some("webm") => "\u{1F3AC}", // video
            Some("pdf") => "\u{1F4C4}",    // document
            Some("doc") | Some("docx") | Some("odt") => "\u{1F4DD}", // memo
            Some("xls") | Some("xlsx") | Some("ods") => "\u{1F4CA}", // chart
            Some("zip") | Some("tar") | Some("gz") | Some("7z") | Some("rar") => "\u{1F4E6}", // package
            Some("rs") | Some("py") | Some("js") | Some("ts") | Some("c") | Some("cpp") | Some("h") => "\u{1F4BB}", // code
            Some("sh") | Some("bash") | Some("zsh") => "\u{1F4BB}", // terminal
            Some("md") | Some("txt") => "\u{1F4C3}", // page
            _ => "\u{1F4C4}", // generic file
        }
    }
}

/// Check if two rectangles intersect.
fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width as i32
        && a.x + a.width as i32 > b.x
        && a.y < b.y + b.height as i32
        && a.y + a.height as i32 > b.y
}
