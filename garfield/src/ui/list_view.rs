//! List view component for displaying directory contents.

use crate::core::{EntryType, FileEntry, SortDirection, SortOrder};
use crate::ui::tab::RenameState;
use gartk_core::{Color, Modifiers, Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::collections::HashSet;

/// Height of each row in the list view.
pub const ROW_HEIGHT: u32 = 28;

/// Height of the header row.
pub const HEADER_HEIGHT: u32 = 28;

/// Minimum column width.
const MIN_COLUMN_WIDTH: u32 = 60;

/// Column identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Size,
    Modified,
}

impl Column {
    /// Convert to SortOrder.
    fn to_sort_order(self) -> SortOrder {
        match self {
            Column::Name => SortOrder::Name,
            Column::Size => SortOrder::Size,
            Column::Modified => SortOrder::Modified,
        }
    }
}

/// List view for displaying file entries.
pub struct ListView {
    /// Entries to display.
    entries: Vec<FileEntry>,
    /// Currently focused index (for keyboard nav).
    focused: usize,
    /// Selected indices (for multi-select).
    selected: HashSet<usize>,
    /// Anchor index for shift-selection.
    selection_anchor: Option<usize>,
    /// Scroll offset (first visible row).
    scroll_offset: usize,
    /// View bounds.
    bounds: Rect,
    /// Show hidden files.
    show_hidden: bool,
    /// Current sort order.
    sort_order: SortOrder,
    /// Current sort direction.
    sort_direction: SortDirection,
    /// Column widths (name, size, modified).
    column_widths: [u32; 3],
    /// Column being resized (if any).
    resizing_column: Option<usize>,
    /// Hovered header column (if any).
    hovered_header: Option<Column>,
}

impl ListView {
    /// Create a new list view.
    pub fn new(bounds: Rect) -> Self {
        // Initial column widths: 50% for name, 100px size, rest for date
        let name_width = (bounds.width as f64 * 0.5) as u32;
        let size_width = 100;
        let date_width = bounds.width.saturating_sub(name_width + size_width + 32);

        Self {
            entries: Vec::new(),
            focused: 0,
            selected: HashSet::new(),
            selection_anchor: None,
            scroll_offset: 0,
            bounds,
            show_hidden: false,
            sort_order: SortOrder::Name,
            sort_direction: SortDirection::Ascending,
            column_widths: [name_width, size_width, date_width],
            resizing_column: None,
            hovered_header: None,
        }
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

    /// Get the focused index.
    pub fn focused_index(&self) -> usize {
        self.focused
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

    /// Check if an index is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected.contains(&index)
    }

    /// Get the number of visible rows that fit in the view.
    fn visible_rows(&self) -> usize {
        let content_height = self.bounds.height.saturating_sub(HEADER_HEIGHT);
        (content_height / ROW_HEIGHT).max(1) as usize
    }

    /// Get current sort settings.
    pub fn sort_settings(&self) -> (SortOrder, SortDirection) {
        (self.sort_order, self.sort_direction)
    }

    /// Toggle hidden files visibility.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let visible_count = self.visible_entries().len();
        if self.focused >= visible_count && visible_count > 0 {
            self.focused = visible_count - 1;
        }
        // Revalidate selection
        self.selected.retain(|&i| i < visible_count);
        if self.selected.is_empty() && visible_count > 0 {
            self.selected.insert(self.focused);
        }
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.focused > 0 {
            self.focused -= 1;
            self.selected.clear();
            self.selected.insert(self.focused);
            self.selection_anchor = Some(self.focused);
            if self.focused < self.scroll_offset {
                self.scroll_offset = self.focused;
            }
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        let visible_count = self.visible_entries().len();
        if self.focused + 1 < visible_count {
            self.focused += 1;
            self.selected.clear();
            self.selected.insert(self.focused);
            self.selection_anchor = Some(self.focused);
            let visible_rows = self.visible_rows();
            if self.focused >= self.scroll_offset + visible_rows {
                self.scroll_offset = self.focused - visible_rows + 1;
            }
        }
    }

    /// Jump to first entry.
    pub fn select_first(&mut self) {
        self.focused = 0;
        self.selected.clear();
        self.selected.insert(0);
        self.selection_anchor = Some(0);
        self.scroll_offset = 0;
    }

    /// Jump to last entry.
    pub fn select_last(&mut self) {
        let visible_count = self.visible_entries().len();
        if visible_count > 0 {
            self.focused = visible_count - 1;
            self.selected.clear();
            self.selected.insert(self.focused);
            self.selection_anchor = Some(self.focused);
            let visible_rows = self.visible_rows();
            if self.focused >= visible_rows {
                self.scroll_offset = self.focused - visible_rows + 1;
            }
        }
    }

    /// Page up.
    pub fn page_up(&mut self) {
        let page_size = self.visible_rows();
        if self.focused >= page_size {
            self.focused -= page_size;
        } else {
            self.focused = 0;
        }
        self.selected.clear();
        self.selected.insert(self.focused);
        self.selection_anchor = Some(self.focused);
        if self.focused < self.scroll_offset {
            self.scroll_offset = self.focused;
        }
    }

    /// Page down.
    pub fn page_down(&mut self) {
        let visible_count = self.visible_entries().len();
        let page_size = self.visible_rows();
        self.focused = (self.focused + page_size).min(visible_count.saturating_sub(1));
        self.selected.clear();
        self.selected.insert(self.focused);
        self.selection_anchor = Some(self.focused);
        if self.focused >= self.scroll_offset + page_size {
            self.scroll_offset = self.focused - page_size + 1;
        }
    }

    /// Select all entries (Ctrl+A).
    pub fn select_all(&mut self) {
        let visible_count = self.visible_entries().len();
        self.selected = (0..visible_count).collect();
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        // Recalculate column widths proportionally
        let total_width = bounds.width.saturating_sub(32);
        let old_total: u32 = self.column_widths.iter().sum();
        if old_total > 0 {
            for width in &mut self.column_widths {
                *width = (*width as f64 / old_total as f64 * total_width as f64) as u32;
            }
        }
    }

    /// Get header bounds.
    fn header_bounds(&self) -> Rect {
        Rect::new(self.bounds.x, self.bounds.y, self.bounds.width, HEADER_HEIGHT)
    }

    /// Get content bounds (below header).
    fn content_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x,
            self.bounds.y + HEADER_HEIGHT as i32,
            self.bounds.width,
            self.bounds.height.saturating_sub(HEADER_HEIGHT),
        )
    }

    /// Get column header bounds for a specific column.
    fn column_header_bounds(&self, col: Column) -> Rect {
        let header = self.header_bounds();
        match col {
            Column::Name => Rect::new(header.x, header.y, self.column_widths[0] + 8, HEADER_HEIGHT),
            Column::Size => Rect::new(
                header.x + self.column_widths[0] as i32 + 16,
                header.y,
                self.column_widths[1],
                HEADER_HEIGHT,
            ),
            Column::Modified => Rect::new(
                header.x + self.column_widths[0] as i32 + self.column_widths[1] as i32 + 24,
                header.y,
                self.column_widths[2],
                HEADER_HEIGHT,
            ),
        }
    }

    /// Get the X position of a column divider.
    fn divider_x(&self, divider_index: usize) -> i32 {
        match divider_index {
            0 => self.bounds.x + self.column_widths[0] as i32 + 12,
            1 => self.bounds.x + self.column_widths[0] as i32 + self.column_widths[1] as i32 + 20,
            _ => 0,
        }
    }

    /// Check if position is near a column divider. Returns divider index (0 or 1) if so.
    pub fn divider_at(&self, pos: Point) -> Option<usize> {
        let header = self.header_bounds();
        if !header.contains_point(pos) {
            return None;
        }

        for i in 0..2 {
            let divider_x = self.divider_x(i);
            if (pos.x - divider_x).abs() < 4 {
                return Some(i);
            }
        }
        None
    }

    /// Start resizing a column divider.
    pub fn start_resize(&mut self, divider_index: usize) {
        self.resizing_column = Some(divider_index);
    }

    /// Stop resizing.
    pub fn stop_resize(&mut self) {
        self.resizing_column = None;
    }

    /// Check if currently resizing.
    pub fn is_resizing(&self) -> bool {
        self.resizing_column.is_some()
    }

    /// Handle mouse move (for hover and resize).
    pub fn on_mouse_move(&mut self, pos: Point) {
        // Handle active resize
        if let Some(divider_index) = self.resizing_column {
            let new_x = pos.x;
            match divider_index {
                0 => {
                    // Resizing between Name and Size columns
                    let new_name_width = (new_x - self.bounds.x - 8).max(MIN_COLUMN_WIDTH as i32) as u32;
                    let total = self.column_widths[0] + self.column_widths[1];
                    let new_size_width = total.saturating_sub(new_name_width).max(MIN_COLUMN_WIDTH);
                    let adjusted_name = total.saturating_sub(new_size_width);
                    if adjusted_name >= MIN_COLUMN_WIDTH {
                        self.column_widths[0] = adjusted_name;
                        self.column_widths[1] = new_size_width;
                    }
                }
                1 => {
                    // Resizing between Size and Modified columns
                    let divider0_x = self.divider_x(0);
                    let new_size_width = (new_x - divider0_x - 8).max(MIN_COLUMN_WIDTH as i32) as u32;
                    let total = self.column_widths[1] + self.column_widths[2];
                    let new_date_width = total.saturating_sub(new_size_width).max(MIN_COLUMN_WIDTH);
                    let adjusted_size = total.saturating_sub(new_date_width);
                    if adjusted_size >= MIN_COLUMN_WIDTH {
                        self.column_widths[1] = adjusted_size;
                        self.column_widths[2] = new_date_width;
                    }
                }
                _ => {}
            }
            return;
        }

        if !self.bounds.contains_point(pos) {
            self.hovered_header = None;
            return;
        }

        let header = self.header_bounds();
        if header.contains_point(pos) {
            // Check for column resize zones
            if self.divider_at(pos).is_some() {
                self.hovered_header = None;
                return;
            }

            // Check column headers for hover
            for col in [Column::Name, Column::Size, Column::Modified] {
                if self.column_header_bounds(col).contains_point(pos) {
                    self.hovered_header = Some(col);
                    return;
                }
            }
        }

        self.hovered_header = None;
    }

    /// Handle header click for sorting. Returns (new_order, new_direction) if sort changed.
    /// Note: Call divider_at() first to check for resize initiation.
    pub fn on_header_click(&mut self, pos: Point) -> Option<(SortOrder, SortDirection)> {
        let header = self.header_bounds();
        if !header.contains_point(pos) {
            return None;
        }

        // Don't sort if clicking on a divider
        if self.divider_at(pos).is_some() {
            return None;
        }

        for col in [Column::Name, Column::Size, Column::Modified] {
            if self.column_header_bounds(col).contains_point(pos) {
                let new_order = col.to_sort_order();
                if self.sort_order == new_order {
                    // Toggle direction
                    self.sort_direction = match self.sort_direction {
                        SortDirection::Ascending => SortDirection::Descending,
                        SortDirection::Descending => SortDirection::Ascending,
                    };
                } else {
                    self.sort_order = new_order;
                    self.sort_direction = SortDirection::Ascending;
                }
                return Some((self.sort_order, self.sort_direction));
            }
        }

        None
    }

    /// Get the entry at the given position (for drag detection).
    pub fn entry_at_point(&self, pos: Point) -> Option<&FileEntry> {
        let content = self.content_bounds();
        if !content.contains_point(pos) {
            return None;
        }

        let relative_y = pos.y - content.y;
        let row_index = self.scroll_offset + (relative_y / ROW_HEIGHT as i32) as usize;

        let visible = self.visible_entries();
        visible.get(row_index).copied()
    }

    /// Handle row click. Returns index of clicked row if valid.
    pub fn on_row_click(&mut self, pos: Point, modifiers: &Modifiers) -> Option<usize> {
        let content = self.content_bounds();
        if !content.contains_point(pos) {
            return None;
        }

        let relative_y = pos.y - content.y;
        let row_index = self.scroll_offset + (relative_y / ROW_HEIGHT as i32) as usize;

        let visible_count = self.visible_entries().len();
        if row_index >= visible_count {
            return None;
        }

        if modifiers.ctrl {
            // Ctrl+click: toggle selection
            if self.selected.contains(&row_index) {
                self.selected.remove(&row_index);
            } else {
                self.selected.insert(row_index);
            }
            self.focused = row_index;
            self.selection_anchor = Some(row_index);
        } else if modifiers.shift {
            // Shift+click: range selection
            if let Some(anchor) = self.selection_anchor {
                let (start, end) = if anchor <= row_index {
                    (anchor, row_index)
                } else {
                    (row_index, anchor)
                };
                self.selected = (start..=end).collect();
            } else {
                self.selected.clear();
                self.selected.insert(row_index);
                self.selection_anchor = Some(row_index);
            }
            self.focused = row_index;
        } else {
            // Plain click: single selection
            self.selected.clear();
            self.selected.insert(row_index);
            self.focused = row_index;
            self.selection_anchor = Some(row_index);
        }

        Some(row_index)
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered_header = None;
    }

    /// Render the list view.
    pub fn render(&self, renderer: &Renderer, rename_state: Option<&RenameState>) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let visible = self.visible_entries();
        let visible_rows = self.visible_rows();

        // Draw header
        self.render_header(renderer)?;

        // Draw entries
        let content = self.content_bounds();
        for (i, entry) in visible
            .iter()
            .skip(self.scroll_offset)
            .take(visible_rows)
            .enumerate()
        {
            let y = content.y + (i as i32 * ROW_HEIGHT as i32);
            let row_rect = Rect::new(content.x, y, content.width, ROW_HEIGHT);

            let actual_index = self.scroll_offset + i;
            let is_selected = self.selected.contains(&actual_index);
            let is_focused = actual_index == self.focused;
            let is_renaming = rename_state.map_or(false, |s| s.index == actual_index);

            // Row background
            if is_selected {
                renderer.fill_rounded_rect(row_rect, 4.0, theme.item_selected_background)?;
            } else if i % 2 == 1 {
                renderer.fill_rect(row_rect, theme.item_background.with_alpha(0.3))?;
            }

            // Focus indicator (subtle border)
            if is_focused && self.selected.len() > 1 {
                renderer.stroke_rect(row_rect, theme.selection_background.with_alpha(0.5), 1.0)?;
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
                .color(if is_selected {
                    theme.selection_foreground
                } else {
                    name_color
                });

            // Name with icon prefix
            let icon = match entry.entry_type {
                EntryType::Directory => "\u{1F4C1} ",
                EntryType::Symlink => "\u{1F517} ",
                _ => "\u{1F4C4} ",
            };

            let name_rect =
                Rect::new(row_rect.x + 8, row_rect.y, self.column_widths[0], ROW_HEIGHT);

            if is_renaming {
                // Render rename text field
                if let Some(state) = rename_state {
                    self.render_rename_field(renderer, name_rect, state, &icon)?;
                }
            } else {
                let display_name = if entry.is_symlink {
                    if let Some(target) = &entry.symlink_target {
                        let target_str = target.to_string_lossy();
                        // Truncate long targets
                        let target_display = if target_str.len() > 30 {
                            format!("...{}", &target_str[target_str.len()-27..])
                        } else {
                            target_str.to_string()
                        };
                        format!("{}{} -> {}", icon, entry.name, target_display)
                    } else {
                        format!("{}{}", icon, entry.name)
                    }
                } else {
                    format!("{}{}", icon, entry.name)
                };

                renderer.text_in_rect(&display_name, name_rect, &name_style)?;
            }

            // Size
            let size_rect = Rect::new(
                row_rect.x + self.column_widths[0] as i32 + 16,
                row_rect.y,
                self.column_widths[1],
                ROW_HEIGHT,
            );
            renderer.text_in_rect(&entry.format_size(), size_rect, &text_style)?;

            // Modified date
            let date_rect = Rect::new(
                row_rect.x + self.column_widths[0] as i32 + self.column_widths[1] as i32 + 24,
                row_rect.y,
                self.column_widths[2],
                ROW_HEIGHT,
            );
            renderer.text_in_rect(&entry.format_modified(), date_rect, &text_style)?;
        }

        Ok(())
    }

    /// Render the inline rename text field.
    fn render_rename_field(&self, renderer: &Renderer, rect: Rect, state: &RenameState, icon: &str) -> anyhow::Result<()> {
        let theme = renderer.theme();

        // Background for text field (slightly lighter)
        let field_rect = Rect::new(
            rect.x + 24, // After icon
            rect.y + 2,
            rect.width.saturating_sub(28),
            rect.height - 4,
        );
        renderer.fill_rounded_rect(field_rect, 2.0, theme.background)?;
        renderer.stroke_rounded_rect(field_rect, 2.0, theme.selection_background, 1.0)?;

        // Draw icon
        let icon_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);
        renderer.text(icon, (rect.x + 4) as f64, (rect.y + 4) as f64, &icon_style)?;

        // Text style for the editable text
        let text_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        // Draw the text
        let text_x = field_rect.x + 4;
        let text_y = field_rect.y + 3;
        renderer.text(&state.text, text_x as f64, text_y as f64, &text_style)?;

        // Draw cursor
        let cursor_x = if state.cursor == 0 {
            text_x as f64
        } else {
            let prefix = &state.text[..state.cursor];
            let prefix_width = renderer.measure_text(prefix, &text_style)?.width;
            text_x as f64 + prefix_width as f64
        };
        renderer.line(
            cursor_x,
            (field_rect.y + 2) as f64,
            cursor_x,
            (field_rect.y + field_rect.height as i32 - 2) as f64,
            theme.foreground,
            1.0,
        )?;

        // Draw selection highlight if any
        if let Some(sel_start) = state.selection_start {
            let (from, to) = if sel_start < state.cursor {
                (sel_start, state.cursor)
            } else {
                (state.cursor, sel_start)
            };

            let from_x = if from == 0 {
                text_x as f64
            } else {
                let prefix = &state.text[..from];
                text_x as f64 + renderer.measure_text(prefix, &text_style)?.width as f64
            };

            let to_x = if to == 0 {
                text_x as f64
            } else {
                let prefix = &state.text[..to];
                text_x as f64 + renderer.measure_text(prefix, &text_style)?.width as f64
            };

            let sel_rect = Rect::new(
                from_x as i32,
                field_rect.y + 2,
                (to_x - from_x) as u32,
                field_rect.height - 4,
            );
            renderer.fill_rect(sel_rect, theme.selection_background.with_alpha(0.3))?;
        }

        Ok(())
    }

    /// Render column headers.
    fn render_header(&self, renderer: &Renderer) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let header = self.header_bounds();

        renderer.fill_rect(header, theme.item_background.darken(0.1))?;

        let header_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground.with_alpha(0.7));

        let hover_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.selection_background);

        // Sort indicator
        let sort_indicator = match self.sort_direction {
            SortDirection::Ascending => " \u{25B2}",  // ▲
            SortDirection::Descending => " \u{25BC}", // ▼
        };

        // Name header
        let name_bounds = self.column_header_bounds(Column::Name);
        let name_style = if self.hovered_header == Some(Column::Name) {
            &hover_style
        } else {
            &header_style
        };
        let name_label = if self.sort_order == SortOrder::Name {
            format!("Name{}", sort_indicator)
        } else {
            "Name".to_string()
        };
        renderer.text_in_rect(&name_label, name_bounds, name_style)?;

        // Size header
        let size_bounds = self.column_header_bounds(Column::Size);
        let size_style = if self.hovered_header == Some(Column::Size) {
            &hover_style
        } else {
            &header_style
        };
        let size_label = if self.sort_order == SortOrder::Size {
            format!("Size{}", sort_indicator)
        } else {
            "Size".to_string()
        };
        renderer.text_in_rect(&size_label, size_bounds, size_style)?;

        // Modified header
        let modified_bounds = self.column_header_bounds(Column::Modified);
        let modified_style = if self.hovered_header == Some(Column::Modified) {
            &hover_style
        } else {
            &header_style
        };
        let modified_label = if self.sort_order == SortOrder::Modified {
            format!("Modified{}", sort_indicator)
        } else {
            "Modified".to_string()
        };
        renderer.text_in_rect(&modified_label, modified_bounds, modified_style)?;

        // Draw column dividers
        let divider_color = theme.border.with_alpha(0.3);
        let divider1_x = (header.x + self.column_widths[0] as i32 + 12) as f64;
        renderer.line(
            divider1_x,
            (header.y + 4) as f64,
            divider1_x,
            (header.y + header.height as i32 - 4) as f64,
            divider_color,
            1.0,
        )?;

        let divider2_x = divider1_x + self.column_widths[1] as f64 + 8.0;
        renderer.line(
            divider2_x,
            (header.y + 4) as f64,
            divider2_x,
            (header.y + header.height as i32 - 4) as f64,
            divider_color,
            1.0,
        )?;

        Ok(())
    }
}
