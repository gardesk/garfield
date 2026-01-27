//! Miller columns view (like macOS Finder).

use crate::core::{read_directory, sort_entries, EntryType, FileEntry, ImagePreview, PdfPreview, SortDirection, SortOrder, is_supported_image, is_pdf};
use gartk_core::{Color, Modifiers, Point, Rect};
use gartk_render::{Renderer, TextStyle, Surface};
use std::collections::HashSet;
use std::path::PathBuf;

/// Height of each row.
const ROW_HEIGHT: u32 = 24;

/// Minimum column width.
const MIN_COLUMN_WIDTH: u32 = 150;

/// Result of a click in the column view.
#[derive(Debug)]
pub enum ColumnClickResult {
    /// Clicked in current column, selected index.
    Selected(usize),
    /// Request to navigate to a path.
    Navigate(PathBuf),
    /// No action.
    None,
}

/// Role of a column for rendering purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnRole {
    /// Parent column - dimmed position indicator.
    Parent,
    /// Current column - active selection highlight.
    Current,
    /// Preview column - no selection highlight.
    Preview,
}

/// A single column in the Miller columns view.
struct Column {
    /// Entries in this column.
    entries: Vec<FileEntry>,
    /// Selected index.
    selected: usize,
    /// Scroll offset.
    scroll_offset: usize,
    /// Hovered index.
    hovered: Option<usize>,
    /// Column bounds.
    bounds: Rect,
}

impl Column {
    fn new(entries: Vec<FileEntry>, bounds: Rect) -> Self {
        Self {
            entries,
            selected: 0,
            scroll_offset: 0,
            hovered: None,
            bounds,
        }
    }

    fn visible_rows(&self) -> usize {
        (self.bounds.height / ROW_HEIGHT).max(1) as usize
    }

    fn visible_entries(&self, show_hidden: bool) -> Vec<&FileEntry> {
        self.entries
            .iter()
            .filter(|e| show_hidden || !e.hidden)
            .collect()
    }
}

/// Miller columns view.
pub struct ColumnView {
    /// Current directory path.
    current_path: PathBuf,
    /// Parent column (one level up).
    parent_column: Option<Column>,
    /// Current column.
    current_column: Column,
    /// Preview column (contents of selected dir or file info).
    preview_column: Option<Column>,
    /// Path that needs preview loading (set when selection changes to a directory).
    pending_preview_path: Option<PathBuf>,
    /// Path that needs image preview loading.
    pending_image_preview_path: Option<PathBuf>,
    /// Loaded image preview.
    image_preview: Option<ImagePreview>,
    /// Path of the loaded image preview (for matching).
    image_preview_path: Option<PathBuf>,
    /// Cached Cairo surface for the image preview.
    image_surface: Option<Surface>,
    /// Path that needs PDF preview loading.
    pending_pdf_preview_path: Option<PathBuf>,
    /// Loaded PDF preview.
    pdf_preview: Option<PdfPreview>,
    /// Path of the loaded PDF preview (for matching).
    pdf_preview_path: Option<PathBuf>,
    /// Cached Cairo surface for the PDF preview.
    pdf_surface: Option<Surface>,
    /// View bounds.
    bounds: Rect,
    /// Show hidden files.
    show_hidden: bool,
    /// Sort order.
    sort_order: SortOrder,
    /// Sort direction.
    sort_direction: SortDirection,
    /// Selected indices for multi-select (in current column).
    selected: HashSet<usize>,
    /// Selection anchor.
    selection_anchor: Option<usize>,
    /// Which column is focused (0=parent, 1=current, 2=preview).
    focused_column: usize,
}

impl ColumnView {
    /// Create a new column view.
    pub fn new(bounds: Rect) -> Self {
        let column_width = bounds.width / 3;
        let column_bounds = Rect::new(bounds.x + column_width as i32, bounds.y, column_width, bounds.height);

        Self {
            current_path: PathBuf::from("/"),
            parent_column: None,
            current_column: Column::new(Vec::new(), column_bounds),
            preview_column: None,
            pending_preview_path: None,
            pending_image_preview_path: None,
            image_preview: None,
            image_preview_path: None,
            image_surface: None,
            pending_pdf_preview_path: None,
            pdf_preview: None,
            pdf_preview_path: None,
            pdf_surface: None,
            bounds,
            show_hidden: false,
            sort_order: SortOrder::Name,
            sort_direction: SortDirection::Ascending,
            selected: HashSet::new(),
            selection_anchor: None,
            focused_column: 1,
        }
    }

    /// Set entries and path for the view.
    pub fn set_entries(&mut self, entries: Vec<FileEntry>) {
        self.current_column.entries = entries;
        self.current_column.selected = 0;
        self.current_column.scroll_offset = 0;
        self.selected.clear();
        self.selected.insert(0);
        self.selection_anchor = Some(0);
        self.focused_column = 1;
        self.update_preview();
    }

    /// Set the current path (for loading parent/preview).
    pub fn set_path(&mut self, path: &PathBuf, sort_order: SortOrder, sort_direction: SortDirection) {
        self.current_path = path.clone();
        self.sort_order = sort_order;
        self.sort_direction = sort_direction;
        self.update_columns();
    }

    /// Update column widths and load data.
    fn update_columns(&mut self) {
        let column_width = self.bounds.width / 3;

        // Load parent directory
        if let Some(parent) = self.current_path.parent() {
            let parent_bounds = Rect::new(self.bounds.x, self.bounds.y, column_width, self.bounds.height);
            let mut entries = read_directory(parent).unwrap_or_default();
            sort_entries(&mut entries, self.sort_order, self.sort_direction);

            // Find index of current dir in parent
            let current_name = self.current_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let selected = entries.iter()
                .position(|e| e.name == current_name)
                .unwrap_or(0);

            let mut parent_col = Column::new(entries, parent_bounds);
            parent_col.selected = selected;
            self.parent_column = Some(parent_col);
        } else {
            self.parent_column = None;
        }

        // Update current column bounds
        let current_x = if self.parent_column.is_some() {
            self.bounds.x + column_width as i32
        } else {
            self.bounds.x
        };
        let current_width = if self.parent_column.is_some() {
            column_width
        } else {
            column_width * 2
        };
        self.current_column.bounds = Rect::new(current_x, self.bounds.y, current_width, self.bounds.height);

        self.update_preview();
    }

    /// Mark that preview needs to be updated based on current selection.
    /// Does not load synchronously - call `take_pending_preview()` to get the path
    /// that needs loading, then `set_preview_entries()` when data is ready.
    fn update_preview(&mut self) {
        let visible = self.current_column.visible_entries(self.show_hidden);
        if let Some(entry) = visible.get(self.current_column.selected).copied() {
            if entry.is_dir() {
                // Check if we already have the correct preview loaded
                let needs_load = self.pending_preview_path.as_ref() != Some(&entry.path);
                if needs_load {
                    self.pending_preview_path = Some(entry.path.clone());
                    // Clear current preview while loading
                    self.preview_column = None;
                }
                // Clear image/PDF preview for directories
                self.clear_file_previews();
            } else {
                self.pending_preview_path = None;
                self.preview_column = None;

                // Check if this is an image file that needs preview
                if is_supported_image(entry.extension().as_deref()) {
                    let needs_load = self.image_preview_path.as_ref() != Some(&entry.path);
                    if needs_load {
                        self.pending_image_preview_path = Some(entry.path.clone());
                        // Clear current image preview while loading
                        self.image_preview = None;
                        self.image_preview_path = None;
                        self.image_surface = None;
                    }
                    // Clear PDF preview when showing image
                    self.pending_pdf_preview_path = None;
                    self.pdf_preview = None;
                    self.pdf_preview_path = None;
                    self.pdf_surface = None;
                } else if is_pdf(entry.extension().as_deref()) {
                    // Check if this is a PDF file that needs preview
                    let needs_load = self.pdf_preview_path.as_ref() != Some(&entry.path);
                    if needs_load {
                        self.pending_pdf_preview_path = Some(entry.path.clone());
                        // Clear current PDF preview while loading
                        self.pdf_preview = None;
                        self.pdf_preview_path = None;
                        self.pdf_surface = None;
                    }
                    // Clear image preview when showing PDF
                    self.pending_image_preview_path = None;
                    self.image_preview = None;
                    self.image_preview_path = None;
                    self.image_surface = None;
                } else {
                    // Not an image or PDF - clear both previews
                    self.clear_file_previews();
                }
            }
        } else {
            self.pending_preview_path = None;
            self.preview_column = None;
            self.clear_file_previews();
        }
    }

    /// Clear all file preview state (image and PDF).
    fn clear_file_previews(&mut self) {
        self.pending_image_preview_path = None;
        self.image_preview = None;
        self.image_preview_path = None;
        self.image_surface = None;
        self.pending_pdf_preview_path = None;
        self.pdf_preview = None;
        self.pdf_preview_path = None;
        self.pdf_surface = None;
    }

    /// Get the path that needs preview loading, if any.
    /// Returns the path and sort settings. Returns None if no preview needed.
    pub fn take_pending_preview(&mut self) -> Option<(PathBuf, SortOrder, SortDirection)> {
        self.pending_preview_path.take().map(|path| {
            (path, self.sort_order, self.sort_direction)
        })
    }

    /// Check if there's a pending preview load for a specific path.
    pub fn has_pending_preview_for(&self, path: &PathBuf) -> bool {
        self.pending_preview_path.as_ref() == Some(path)
    }

    /// Set preview entries from externally loaded data.
    pub fn set_preview_entries(&mut self, path: &PathBuf, entries: Vec<FileEntry>) {
        // Only set if this is still the path we're waiting for
        let visible = self.current_column.visible_entries(self.show_hidden);
        let selected_is_dir = visible
            .get(self.current_column.selected)
            .map(|e| e.is_dir() && &e.path == path)
            .unwrap_or(false);

        if selected_is_dir {
            let preview_x = self.current_column.bounds.x + self.current_column.bounds.width as i32;
            let preview_width = self.bounds.x + self.bounds.width as i32 - preview_x;
            let preview_bounds = Rect::new(preview_x, self.bounds.y, preview_width as u32, self.bounds.height);
            self.preview_column = Some(Column::new(entries, preview_bounds));
        }
        // Clear pending if this was what we were waiting for
        if self.pending_preview_path.as_ref() == Some(path) {
            self.pending_preview_path = None;
        }
    }

    /// Check if preview is currently loading.
    pub fn is_preview_loading(&self) -> bool {
        self.pending_preview_path.is_some()
    }

    /// Take pending image preview request (path, max_width, max_height).
    pub fn take_pending_image_preview(&mut self) -> Option<(PathBuf, u32, u32)> {
        self.pending_image_preview_path.take().map(|path| {
            let preview_x = self.current_column.bounds.x + self.current_column.bounds.width as i32;
            let preview_width = (self.bounds.x + self.bounds.width as i32 - preview_x) as u32;
            let preview_height = self.bounds.height / 2; // Use half height for image
            (path, preview_width.saturating_sub(32), preview_height.saturating_sub(32))
        })
    }

    /// Set loaded image preview.
    pub fn set_image_preview(&mut self, path: &PathBuf, image: Option<ImagePreview>) {
        // Only set if this is still the path we're displaying
        let visible = self.current_column.visible_entries(self.show_hidden);
        let selected_matches = visible
            .get(self.current_column.selected)
            .map(|e| &e.path == path)
            .unwrap_or(false);

        if selected_matches {
            if let Some(ref preview) = image {
                // Create a Cairo surface from the RGBA data
                self.image_surface = Surface::from_rgba(&preview.data, preview.width, preview.height).ok();
            } else {
                self.image_surface = None;
            }
            self.image_preview = image;
            self.image_preview_path = Some(path.clone());
        }
    }

    /// Take pending PDF preview request (path, max_width, max_height).
    pub fn take_pending_pdf_preview(&mut self) -> Option<(PathBuf, u32, u32)> {
        self.pending_pdf_preview_path.take().map(|path| {
            let preview_x = self.current_column.bounds.x + self.current_column.bounds.width as i32;
            let preview_width = (self.bounds.x + self.bounds.width as i32 - preview_x) as u32;
            let preview_height = self.bounds.height / 2; // Use half height for PDF
            (path, preview_width.saturating_sub(32), preview_height.saturating_sub(32))
        })
    }

    /// Set loaded PDF preview.
    pub fn set_pdf_preview(&mut self, path: &PathBuf, pdf: Option<PdfPreview>) {
        // Only set if this is still the path we're displaying
        let visible = self.current_column.visible_entries(self.show_hidden);
        let selected_matches = visible
            .get(self.current_column.selected)
            .map(|e| &e.path == path)
            .unwrap_or(false);

        if selected_matches {
            if let Some(ref preview) = pdf {
                // Create a Cairo surface from the RGBA data
                self.pdf_surface = Surface::from_rgba(&preview.data, preview.width, preview.height).ok();
            } else {
                self.pdf_surface = None;
            }
            self.pdf_preview = pdf;
            self.pdf_preview_path = Some(path.clone());
        }
    }

    /// Get visible entries in current column.
    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        self.current_column.visible_entries(self.show_hidden)
    }

    /// Get the currently selected entry.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        let visible = self.visible_entries();
        visible.get(self.current_column.selected).copied()
    }

    /// Get the focused index.
    pub fn focused_index(&self) -> usize {
        self.current_column.selected
    }

    /// Set the focused index and select it.
    pub fn set_focused(&mut self, index: usize) {
        if index < self.current_column.entries.len() {
            self.current_column.selected = index;
            self.selected.clear();
            self.selected.insert(index);
        }
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

    /// Get current sort settings.
    pub fn sort_settings(&self) -> (SortOrder, SortDirection) {
        (self.sort_order, self.sort_direction)
    }

    /// Toggle hidden files visibility.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let visible_count = self.visible_entries().len();
        if self.current_column.selected >= visible_count && visible_count > 0 {
            self.current_column.selected = visible_count - 1;
        }
        self.selected.retain(|&i| i < visible_count);
        if self.selected.is_empty() && visible_count > 0 {
            self.selected.insert(self.current_column.selected);
        }
        self.update_preview();
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.current_column.selected > 0 {
            self.current_column.selected -= 1;
            self.update_single_selection();
            self.update_preview();
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        let visible_count = self.visible_entries().len();
        if self.current_column.selected + 1 < visible_count {
            self.current_column.selected += 1;
            self.update_single_selection();
            self.update_preview();
        }
    }

    /// Update selection to single focused item.
    fn update_single_selection(&mut self) {
        self.selected.clear();
        self.selected.insert(self.current_column.selected);
        self.selection_anchor = Some(self.current_column.selected);
        self.ensure_visible();
    }

    /// Ensure current selection is visible.
    fn ensure_visible(&mut self) {
        let visible_rows = self.current_column.visible_rows();
        if self.current_column.selected < self.current_column.scroll_offset {
            self.current_column.scroll_offset = self.current_column.selected;
        } else if self.current_column.selected >= self.current_column.scroll_offset + visible_rows {
            self.current_column.scroll_offset = self.current_column.selected - visible_rows + 1;
        }
    }

    /// Jump to first entry.
    pub fn select_first(&mut self) {
        self.current_column.selected = 0;
        self.update_single_selection();
        self.update_preview();
    }

    /// Jump to last entry.
    pub fn select_last(&mut self) {
        let visible_count = self.visible_entries().len();
        if visible_count > 0 {
            self.current_column.selected = visible_count - 1;
            self.update_single_selection();
            self.update_preview();
        }
    }

    /// Page up.
    pub fn page_up(&mut self) {
        let page_size = self.current_column.visible_rows();
        if self.current_column.selected >= page_size {
            self.current_column.selected -= page_size;
        } else {
            self.current_column.selected = 0;
        }
        self.update_single_selection();
        self.update_preview();
    }

    /// Page down.
    pub fn page_down(&mut self) {
        let visible_count = self.visible_entries().len();
        let page_size = self.current_column.visible_rows();
        self.current_column.selected = (self.current_column.selected + page_size).min(visible_count.saturating_sub(1));
        self.update_single_selection();
        self.update_preview();
    }

    /// Select all entries (Ctrl+A).
    pub fn select_all(&mut self) {
        let visible_count = self.visible_entries().len();
        self.selected = (0..visible_count).collect();
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.update_columns();
    }

    /// Handle mouse move for hover effects. Returns true if hover state changed.
    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        let mut changed = false;

        // Parent column hover
        if let Some(ref mut parent) = self.parent_column {
            let old_hovered = parent.hovered;
            parent.hovered = None;
            if parent.bounds.contains_point(pos) {
                let visible = parent.visible_entries(self.show_hidden);
                for i in parent.scroll_offset..(parent.scroll_offset + parent.visible_rows()).min(visible.len()) {
                    let y = parent.bounds.y + ((i - parent.scroll_offset) as i32 * ROW_HEIGHT as i32);
                    let row = Rect::new(parent.bounds.x, y, parent.bounds.width, ROW_HEIGHT);
                    if row.contains_point(pos) {
                        parent.hovered = Some(i);
                        break;
                    }
                }
            }
            if parent.hovered != old_hovered {
                changed = true;
            }
        }

        // Current column hover
        let old_current_hovered = self.current_column.hovered;
        self.current_column.hovered = None;
        if self.current_column.bounds.contains_point(pos) {
            let visible = self.visible_entries();
            for i in self.current_column.scroll_offset..(self.current_column.scroll_offset + self.current_column.visible_rows()).min(visible.len()) {
                let y = self.current_column.bounds.y + ((i - self.current_column.scroll_offset) as i32 * ROW_HEIGHT as i32);
                let row = Rect::new(self.current_column.bounds.x, y, self.current_column.bounds.width, ROW_HEIGHT);
                if row.contains_point(pos) {
                    self.current_column.hovered = Some(i);
                    break;
                }
            }
        }
        if self.current_column.hovered != old_current_hovered {
            changed = true;
        }

        // Preview column hover
        if let Some(ref mut preview) = self.preview_column {
            let old_preview_hovered = preview.hovered;
            preview.hovered = None;
            if preview.bounds.contains_point(pos) {
                let visible = preview.visible_entries(self.show_hidden);
                for i in preview.scroll_offset..(preview.scroll_offset + preview.visible_rows()).min(visible.len()) {
                    let y = preview.bounds.y + ((i - preview.scroll_offset) as i32 * ROW_HEIGHT as i32);
                    let row = Rect::new(preview.bounds.x, y, preview.bounds.width, ROW_HEIGHT);
                    if row.contains_point(pos) {
                        preview.hovered = Some(i);
                        break;
                    }
                }
            }
            if preview.hovered != old_preview_hovered {
                changed = true;
            }
        }

        changed
    }

    /// Handle mouse scroll. Returns true if scrolled.
    pub fn on_scroll(&mut self, delta_y: i32) -> bool {
        // Scroll the current column
        let visible = self.visible_entries();
        let total_rows = visible.len();
        let visible_rows = self.current_column.visible_rows();

        if total_rows <= visible_rows {
            return false;
        }

        let max_scroll = total_rows.saturating_sub(visible_rows);
        let old_offset = self.current_column.scroll_offset;

        if delta_y < 0 {
            let rows = ((-delta_y) as usize / 3).max(1);
            self.current_column.scroll_offset = self.current_column.scroll_offset.saturating_sub(rows);
        } else if delta_y > 0 {
            let rows = (delta_y as usize / 3).max(1);
            self.current_column.scroll_offset = (self.current_column.scroll_offset + rows).min(max_scroll);
        }

        self.current_column.scroll_offset != old_offset
    }

    /// Get the entry at the given position (for drag detection).
    pub fn entry_at_point(&self, pos: Point) -> Option<&FileEntry> {
        // Check current column (main selection column)
        if self.current_column.bounds.contains_point(pos) {
            let visible = self.visible_entries();
            let visible_rows = self.current_column.visible_rows();

            for i in self.current_column.scroll_offset..(self.current_column.scroll_offset + visible_rows).min(visible.len()) {
                let y = self.current_column.bounds.y + ((i - self.current_column.scroll_offset) as i32 * ROW_HEIGHT as i32);
                let row = Rect::new(self.current_column.bounds.x, y, self.current_column.bounds.width, ROW_HEIGHT);

                if row.contains_point(pos) {
                    return visible.get(i).copied();
                }
            }
        }

        // Check parent column
        if let Some(ref parent) = self.parent_column {
            if parent.bounds.contains_point(pos) {
                let visible = parent.visible_entries(self.show_hidden);
                let visible_rows = parent.visible_rows();

                for i in parent.scroll_offset..(parent.scroll_offset + visible_rows).min(visible.len()) {
                    let y = parent.bounds.y + ((i - parent.scroll_offset) as i32 * ROW_HEIGHT as i32);
                    let row = Rect::new(parent.bounds.x, y, parent.bounds.width, ROW_HEIGHT);

                    if row.contains_point(pos) {
                        return visible.get(i).copied();
                    }
                }
            }
        }

        // Check preview column
        if let Some(ref preview) = self.preview_column {
            if preview.bounds.contains_point(pos) {
                let visible = preview.visible_entries(self.show_hidden);
                let visible_rows = preview.visible_rows();

                for i in preview.scroll_offset..(preview.scroll_offset + visible_rows).min(visible.len()) {
                    let y = preview.bounds.y + ((i - preview.scroll_offset) as i32 * ROW_HEIGHT as i32);
                    let row = Rect::new(preview.bounds.x, y, preview.bounds.width, ROW_HEIGHT);

                    if row.contains_point(pos) {
                        return visible.get(i).copied();
                    }
                }
            }
        }

        None
    }

    /// Handle click in any column. Returns click result.
    pub fn on_click(&mut self, pos: Point, modifiers: &Modifiers) -> ColumnClickResult {
        // Check parent column click - navigate to clicked directory
        if let Some(ref parent) = self.parent_column {
            if parent.bounds.contains_point(pos) {
                let visible = parent.visible_entries(self.show_hidden);
                let visible_rows = parent.visible_rows();

                for i in parent.scroll_offset..(parent.scroll_offset + visible_rows).min(visible.len()) {
                    let y = parent.bounds.y + ((i - parent.scroll_offset) as i32 * ROW_HEIGHT as i32);
                    let row = Rect::new(parent.bounds.x, y, parent.bounds.width, ROW_HEIGHT);

                    if row.contains_point(pos) {
                        if let Some(entry) = visible.get(i) {
                            if entry.is_dir() {
                                return ColumnClickResult::Navigate(entry.path.clone());
                            }
                        }
                        return ColumnClickResult::None;
                    }
                }
                return ColumnClickResult::None;
            }
        }

        // Check preview column click - navigate into clicked directory
        if let Some(ref preview) = self.preview_column {
            if preview.bounds.contains_point(pos) {
                let visible = preview.visible_entries(self.show_hidden);
                let visible_rows = preview.visible_rows();

                for i in preview.scroll_offset..(preview.scroll_offset + visible_rows).min(visible.len()) {
                    let y = preview.bounds.y + ((i - preview.scroll_offset) as i32 * ROW_HEIGHT as i32);
                    let row = Rect::new(preview.bounds.x, y, preview.bounds.width, ROW_HEIGHT);

                    if row.contains_point(pos) {
                        if let Some(entry) = visible.get(i) {
                            if entry.is_dir() {
                                return ColumnClickResult::Navigate(entry.path.clone());
                            }
                        }
                        return ColumnClickResult::None;
                    }
                }
                return ColumnClickResult::None;
            }
        }

        // Check current column click
        if !self.current_column.bounds.contains_point(pos) {
            return ColumnClickResult::None;
        }

        self.focused_column = 1;
        let visible = self.visible_entries();
        let visible_rows = self.current_column.visible_rows();

        for i in self.current_column.scroll_offset..(self.current_column.scroll_offset + visible_rows).min(visible.len()) {
            let y = self.current_column.bounds.y + ((i - self.current_column.scroll_offset) as i32 * ROW_HEIGHT as i32);
            let row = Rect::new(self.current_column.bounds.x, y, self.current_column.bounds.width, ROW_HEIGHT);

            if row.contains_point(pos) {
                if modifiers.ctrl {
                    if self.selected.contains(&i) {
                        self.selected.remove(&i);
                    } else {
                        self.selected.insert(i);
                    }
                    self.current_column.selected = i;
                    self.selection_anchor = Some(i);
                } else if modifiers.shift {
                    if let Some(anchor) = self.selection_anchor {
                        let (start, end) = if anchor <= i { (anchor, i) } else { (i, anchor) };
                        self.selected = (start..=end).collect();
                    } else {
                        self.selected.clear();
                        self.selected.insert(i);
                        self.selection_anchor = Some(i);
                    }
                    self.current_column.selected = i;
                } else {
                    self.selected.clear();
                    self.selected.insert(i);
                    self.current_column.selected = i;
                    self.selection_anchor = Some(i);
                }
                self.update_preview();
                return ColumnClickResult::Selected(i);
            }
        }

        ColumnClickResult::None
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        if let Some(ref mut parent) = self.parent_column {
            parent.hovered = None;
        }
        self.current_column.hovered = None;
        if let Some(ref mut preview) = self.preview_column {
            preview.hovered = None;
        }
    }

    /// Render the column view.
    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        let theme = renderer.theme();

        // Draw parent column (with dimmed position indicator)
        if let Some(ref parent) = self.parent_column {
            self.render_column(renderer, parent, ColumnRole::Parent)?;
            // Draw divider
            let divider_x = parent.bounds.x + parent.bounds.width as i32;
            renderer.line(
                divider_x as f64,
                self.bounds.y as f64,
                divider_x as f64,
                (self.bounds.y + self.bounds.height as i32) as f64,
                theme.border,
                1.0,
            )?;
        }

        // Draw current column (with active selection highlight)
        self.render_column(renderer, &self.current_column, ColumnRole::Current)?;

        // Draw preview column (no selection highlight)
        if let Some(ref preview) = self.preview_column {
            let divider_x = self.current_column.bounds.x + self.current_column.bounds.width as i32;
            renderer.line(
                divider_x as f64,
                self.bounds.y as f64,
                divider_x as f64,
                (self.bounds.y + self.bounds.height as i32) as f64,
                theme.border,
                1.0,
            )?;
            self.render_column(renderer, preview, ColumnRole::Preview)?;
        } else if let Some(entry) = self.selected_entry() {
            // Show file info for non-directories
            if !entry.is_dir() {
                self.render_file_info(renderer, entry)?;
            }
        }

        Ok(())
    }

    /// Render a single column.
    fn render_column(&self, renderer: &Renderer, column: &Column, role: ColumnRole) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let visible = column.visible_entries(self.show_hidden);
        let visible_rows = column.visible_rows();

        for (i, entry) in visible.iter()
            .skip(column.scroll_offset)
            .take(visible_rows)
            .enumerate()
        {
            let actual_index = column.scroll_offset + i;
            let y = column.bounds.y + (i as i32 * ROW_HEIGHT as i32);
            let row = Rect::new(column.bounds.x, y, column.bounds.width, ROW_HEIGHT);

            // Determine selection state based on column role
            let (is_selected, show_highlight) = match role {
                ColumnRole::Current => (self.selected.contains(&actual_index), true),
                ColumnRole::Parent => (actual_index == column.selected, true),
                ColumnRole::Preview => (false, false), // No highlight in preview
            };
            let is_hovered = column.hovered == Some(actual_index);

            // Row background
            if is_selected && show_highlight {
                if role == ColumnRole::Parent {
                    // Dimmer highlight for parent column position indicator
                    renderer.fill_rect(row, theme.item_selected_background.with_alpha(0.4))?;
                } else {
                    // Full highlight for current column
                    renderer.fill_rect(row, theme.item_selected_background)?;
                }
            } else if is_hovered {
                renderer.fill_rect(row, theme.item_background)?;
            }

            // Entry color - don't use selection foreground for parent column
            let text_color = if is_selected && show_highlight && role == ColumnRole::Current {
                theme.selection_foreground
            } else if entry.hidden {
                theme.item_foreground.with_alpha(0.5)
            } else {
                match entry.entry_type {
                    EntryType::Directory => Color::from_hex("#5c9fd8").unwrap_or(theme.item_foreground),
                    EntryType::Symlink => Color::from_hex("#c678dd").unwrap_or(theme.item_foreground),
                    _ => theme.item_foreground,
                }
            };

            let text_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size - 1.0)
                .color(text_color);

            // Directory/symlink indicator
            let prefix = if entry.is_navigable() { "> " } else { "  " };
            let suffix = if entry.is_symlink { " @" } else { "" };
            let display_text = format!("{}{}{}", prefix, entry.name, suffix);

            let text_rect = Rect::new(row.x + 4, row.y, row.width - 8, ROW_HEIGHT);
            renderer.text_in_rect(&display_text, text_rect, &text_style)?;
        }

        Ok(())
    }

    /// Render file info panel for non-directory files.
    fn render_file_info(&self, renderer: &Renderer, entry: &FileEntry) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let preview_x = self.current_column.bounds.x + self.current_column.bounds.width as i32;
        let preview_width = (self.bounds.x + self.bounds.width as i32 - preview_x) as u32;

        // Draw divider
        renderer.line(
            preview_x as f64,
            self.bounds.y as f64,
            preview_x as f64,
            (self.bounds.y + self.bounds.height as i32) as f64,
            theme.border,
            1.0,
        )?;

        let mut y = self.bounds.y + 16;
        let x = preview_x + 16;

        // Render image preview if available
        if let Some(ref surface) = self.image_surface {
            // Center the image in the preview area
            let img_width = surface.width();
            let img_height = surface.height();
            let max_width = preview_width.saturating_sub(32);

            let img_x = preview_x + 16 + (max_width.saturating_sub(img_width) / 2) as i32;
            let img_y = y;

            // Draw the image using Cairo
            let ctx = renderer.context()?;
            ctx.set_source_surface(surface.cairo_surface(), img_x as f64, img_y as f64)?;
            ctx.paint()?;

            // Move y below the image with padding
            y += img_height as i32 + 24;
        } else if let Some(ref surface) = self.pdf_surface {
            // Render PDF preview
            let img_width = surface.width();
            let img_height = surface.height();
            let max_width = preview_width.saturating_sub(32);

            let img_x = preview_x + 16 + (max_width.saturating_sub(img_width) / 2) as i32;
            let img_y = y;

            // Draw the PDF preview using Cairo
            let ctx = renderer.context()?;
            ctx.set_source_surface(surface.cairo_surface(), img_x as f64, img_y as f64)?;
            ctx.paint()?;

            // Show page count if available
            if let Some(ref pdf) = self.pdf_preview {
                let page_info = format!("Page 1 of {}", pdf.page_count);
                let page_style = TextStyle::new()
                    .font_family(&theme.font_family)
                    .font_size(theme.font_size - 2.0)
                    .color(theme.item_foreground.with_alpha(0.6));
                renderer.text(&page_info, (img_x + 4) as f64, (img_y + img_height as i32 + 4) as f64, &page_style)?;
            }

            // Move y below the preview with padding
            y += img_height as i32 + 32;
        } else if is_supported_image(entry.extension().as_deref()) && self.pending_image_preview_path.is_some() {
            // Show loading indicator for images
            let loading_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size)
                .color(theme.item_foreground.with_alpha(0.5));
            renderer.text("Loading preview...", x as f64, y as f64, &loading_style)?;
            y += 40;
        } else if is_pdf(entry.extension().as_deref()) && self.pending_pdf_preview_path.is_some() {
            // Show loading indicator for PDFs
            let loading_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size)
                .color(theme.item_foreground.with_alpha(0.5));
            renderer.text("Loading PDF preview...", x as f64, y as f64, &loading_style)?;
            y += 40;
        }

        let label_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.item_foreground.with_alpha(0.6));

        let value_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);

        // File name
        renderer.text("Name", x as f64, y as f64, &label_style)?;
        y += 18;
        renderer.text(&entry.name, x as f64, y as f64, &value_style)?;
        y += 32;

        // Size
        renderer.text("Size", x as f64, y as f64, &label_style)?;
        y += 18;
        renderer.text(&entry.format_size(), x as f64, y as f64, &value_style)?;
        y += 32;

        // Modified
        renderer.text("Modified", x as f64, y as f64, &label_style)?;
        y += 18;
        renderer.text(&entry.format_modified(), x as f64, y as f64, &value_style)?;
        y += 32;

        // Type
        renderer.text("Type", x as f64, y as f64, &label_style)?;
        y += 18;
        let file_type = entry.extension()
            .map(|e| e.to_uppercase())
            .unwrap_or_else(|| "File".to_string());
        renderer.text(&file_type, x as f64, y as f64, &value_style)?;

        Ok(())
    }
}
