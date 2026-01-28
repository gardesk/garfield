//! Grid/icon view for displaying directory contents.

use crate::core::{is_supported_image, is_pdf, EntryType, FileEntry, SortDirection, SortOrder, ThumbnailLoader};
use crate::ui::tab::RenameState;
use gartk_core::{Color, Modifiers, Point, Rect};
use gartk_render::{Renderer, Surface, TextAlign, TextStyle};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Padding around cells.
pub const CELL_PADDING: u32 = 8;

/// Icon size setting for grid view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl IconSize {
    /// Get the cell size for this icon size.
    pub fn cell_size(&self) -> u32 {
        match self {
            IconSize::Small => 70,
            IconSize::Medium => 100,
            IconSize::Large => 140,
        }
    }

    /// Get the icon size within the cell.
    pub fn icon_size(&self) -> u32 {
        match self {
            IconSize::Small => 32,
            IconSize::Medium => 48,
            IconSize::Large => 64,
        }
    }

    /// Get the font size for labels.
    pub fn font_size(&self) -> f64 {
        match self {
            IconSize::Small => 10.0,
            IconSize::Medium => 12.0,
            IconSize::Large => 14.0,
        }
    }

    /// Cycle to the next (larger) size.
    pub fn next(&self) -> Self {
        match self {
            IconSize::Small => IconSize::Medium,
            IconSize::Medium => IconSize::Large,
            IconSize::Large => IconSize::Large, // Already at max
        }
    }

    /// Cycle to the previous (smaller) size.
    pub fn prev(&self) -> Self {
        match self {
            IconSize::Small => IconSize::Small, // Already at min
            IconSize::Medium => IconSize::Small,
            IconSize::Large => IconSize::Medium,
        }
    }

    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            IconSize::Small => "Small",
            IconSize::Medium => "Medium",
            IconSize::Large => "Large",
        }
    }
}

/// Grid view for displaying file entries as icons.
pub struct GridView {
    /// Entries to display.
    entries: Vec<FileEntry>,
    /// Cached indices of visible entries (respecting hidden filter).
    visible_indices: Vec<usize>,
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
    /// Icon size setting.
    icon_size: IconSize,
    /// Last time rubber band selection was updated (for throttling).
    last_selection_update: Option<Instant>,
    /// Last time we requested a redraw during drag (for frame rate limiting).
    last_drag_render: Option<Instant>,
    /// Thumbnail loader for image files.
    thumbnail_loader: ThumbnailLoader,
    /// Cached thumbnail surfaces.
    thumbnail_cache: HashMap<PathBuf, Surface>,
}

impl GridView {
    /// Create a new grid view.
    pub fn new(bounds: Rect) -> Self {
        let icon_size = IconSize::default();
        let columns = Self::calculate_columns_for_size(bounds.width, icon_size);
        Self {
            entries: Vec::new(),
            visible_indices: Vec::new(),
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
            icon_size,
            last_selection_update: None,
            last_drag_render: None,
            thumbnail_loader: ThumbnailLoader::new(),
            thumbnail_cache: HashMap::new(),
        }
    }

    /// Rebuild the visible indices cache.
    fn rebuild_visible_cache(&mut self) {
        self.visible_indices = self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_hidden || !e.hidden)
            .map(|(i, _)| i)
            .collect();
    }

    /// Calculate number of columns that fit in the given width for a specific icon size.
    fn calculate_columns_for_size(width: u32, icon_size: IconSize) -> usize {
        let cell_size = icon_size.cell_size();
        ((width - CELL_PADDING) / (cell_size + CELL_PADDING)).max(1) as usize
    }

    /// Calculate number of columns that fit in the given width.
    fn calculate_columns(&self, width: u32) -> usize {
        Self::calculate_columns_for_size(width, self.icon_size)
    }

    /// Get the current icon size.
    pub fn icon_size(&self) -> IconSize {
        self.icon_size
    }

    /// Set the icon size.
    pub fn set_icon_size(&mut self, size: IconSize) {
        self.icon_size = size;
        self.columns = self.calculate_columns(self.bounds.width);
    }

    /// Cycle to the next icon size.
    pub fn cycle_icon_size(&mut self) {
        self.set_icon_size(self.icon_size.next());
    }

    /// Increase icon size (to next larger).
    pub fn increase_icon_size(&mut self) {
        self.set_icon_size(self.icon_size.next());
    }

    /// Decrease icon size (to next smaller).
    pub fn decrease_icon_size(&mut self) {
        self.set_icon_size(self.icon_size.prev());
    }

    /// Set the entries to display.
    pub fn set_entries(&mut self, entries: Vec<FileEntry>) {
        self.entries = entries;
        self.rebuild_visible_cache();
        self.focused = 0;
        self.selected.clear();
        self.selected.insert(0);
        self.selection_anchor = Some(0);
        self.scroll_offset = 0;
        // Clear thumbnail cache when directory changes
        self.thumbnail_cache.clear();
        self.thumbnail_loader.clear_cache();
    }

    /// Poll for completed thumbnail loads. Returns true if any thumbnails were loaded.
    pub fn poll_thumbnails(&mut self) -> bool {
        let loaded = self.thumbnail_loader.poll();
        let mut any_loaded = false;

        for path in loaded {
            // Create surface from cached thumbnail (borrow ends after and_then)
            let surface_opt = self.thumbnail_loader
                .get_cached(&path)
                .and_then(|thumbnail| {
                    Surface::from_rgba(&thumbnail.data, thumbnail.width, thumbnail.height).ok()
                });

            // Now insert (no borrow of thumbnail_loader active)
            if let Some(surface) = surface_opt {
                self.thumbnail_cache.insert(path, surface);
                any_loaded = true;
            }
        }

        any_loaded
    }

    /// Request thumbnails for visible image and PDF files.
    pub fn request_visible_thumbnails(&mut self) {
        let visible_count = self.visible_count();
        let visible_rows = self.visible_rows();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + visible_rows * self.columns).min(visible_count);

        // Collect paths to load (immutable borrow of entries)
        let paths_to_load: Vec<PathBuf> = (start_index..end_index)
            .filter_map(|i| {
                self.visible_entry(i).and_then(|entry| {
                    let ext_owned = entry.extension();
                    let ext = ext_owned.as_deref();
                    let is_thumbnailable = is_supported_image(ext) || is_pdf(ext);
                    if is_thumbnailable && !self.thumbnail_cache.contains_key(&entry.path) {
                        Some(entry.path.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Now request loading (mutable borrow of thumbnail_loader)
        for path in paths_to_load {
            self.thumbnail_loader.get_or_load(&path);
        }
    }

    /// Get visible entries (respecting hidden filter).
    /// Uses cached indices for efficiency.
    pub fn visible_entries(&self) -> Vec<&FileEntry> {
        self.visible_indices
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .collect()
    }

    /// Get visible entry count without allocation.
    #[inline]
    pub fn visible_count(&self) -> usize {
        self.visible_indices.len()
    }

    /// Get entry at visible index without allocation.
    #[inline]
    pub fn visible_entry(&self, visible_idx: usize) -> Option<&FileEntry> {
        self.visible_indices.get(visible_idx).and_then(|&i| self.entries.get(i))
    }

    /// Get the currently focused entry.
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.visible_entry(self.focused)
    }

    /// Get the focused index.
    pub fn focused_index(&self) -> usize {
        self.focused
    }

    /// Set the focused index and select it.
    pub fn set_focused(&mut self, index: usize) {
        if index < self.entries.len() {
            self.focused = index;
            self.selected.clear();
            self.selected.insert(index);
        }
    }

    /// Get all selected entries.
    pub fn selected_entries(&self) -> Vec<&FileEntry> {
        self.selected
            .iter()
            .filter_map(|&i| self.visible_entry(i))
            .collect()
    }

    /// Get selection count.
    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    /// Get the number of visible rows that fit in the view.
    fn visible_rows(&self) -> usize {
        let cell_size = self.icon_size.cell_size();
        (self.bounds.height / (cell_size + CELL_PADDING)).max(1) as usize
    }

    /// Get current sort settings (grid view doesn't track these, uses external).
    pub fn sort_settings(&self) -> (SortOrder, SortDirection) {
        (SortOrder::Name, SortDirection::Ascending)
    }

    /// Toggle hidden files visibility.
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.rebuild_visible_cache();
        let visible_count = self.visible_count();
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
        let visible_count = self.visible_count();
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
        let visible_count = self.visible_count();
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
        let visible_count = self.visible_count();
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
        let visible_count = self.visible_count();
        let page_size = self.visible_rows() * self.columns;
        self.focused = (self.focused + page_size).min(visible_count.saturating_sub(1));
        self.update_single_selection();
    }

    /// Select all entries (Ctrl+A).
    pub fn select_all(&mut self) {
        let visible_count = self.visible_count();
        self.selected = (0..visible_count).collect();
    }

    /// Clear the selection.
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.columns = self.calculate_columns(bounds.width);
    }

    /// Get cell bounds for an index.
    fn cell_bounds(&self, index: usize) -> Rect {
        let visible_index = index.saturating_sub(self.scroll_offset * self.columns);
        let col = visible_index % self.columns;
        let row = visible_index / self.columns;
        let cell_size = self.icon_size.cell_size();

        let x = self.bounds.x + CELL_PADDING as i32 + (col as i32 * (cell_size + CELL_PADDING) as i32);
        let y = self.bounds.y + CELL_PADDING as i32 + (row as i32 * (cell_size + CELL_PADDING) as i32);

        Rect::new(x, y, cell_size, cell_size)
    }

    /// Handle mouse move for hover effects and rubber band drag. Returns true if state changed.
    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        // Handle rubber band drag
        if self.drag_start.is_some() {
            self.drag_current = Some(pos);

            // Throttle both selection updates AND render requests to ~60fps
            let should_update = match self.last_selection_update {
                Some(last) => last.elapsed() >= Duration::from_millis(16),
                None => true,
            };

            if should_update {
                self.update_rubber_band_selection();
                self.last_selection_update = Some(Instant::now());
                self.last_drag_render = Some(Instant::now());
                return true; // Request redraw at throttled rate
            }

            // Also limit render requests independently (in case selection didn't change)
            let should_render = match self.last_drag_render {
                Some(last) => last.elapsed() >= Duration::from_millis(16),
                None => true,
            };

            if should_render {
                self.last_drag_render = Some(Instant::now());
                return true;
            }

            return false; // Skip redraw, we're within throttle window
        }

        let old_hovered = self.hovered;

        if !self.bounds.contains_point(pos) {
            self.hovered = None;
            return self.hovered != old_hovered;
        }

        let visible_count = self.visible_count();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + self.visible_rows() * self.columns).min(visible_count);

        self.hovered = None;
        for i in start_index..end_index {
            if self.cell_bounds(i).contains_point(pos) {
                self.hovered = Some(i);
                break;
            }
        }

        self.hovered != old_hovered
    }

    /// Handle mouse scroll. Returns true if scrolled.
    pub fn on_scroll(&mut self, delta_y: i32) -> bool {
        let visible_count = self.visible_count();
        let total_rows = (visible_count + self.columns - 1) / self.columns;
        let visible_rows = self.visible_rows();

        if total_rows <= visible_rows {
            return false; // No scrolling needed
        }

        let max_scroll = total_rows.saturating_sub(visible_rows);
        let old_offset = self.scroll_offset;

        // Scroll by number of rows (negative delta = scroll up)
        if delta_y < 0 {
            // Scroll up
            let rows = ((-delta_y) as usize / 3).max(1);
            self.scroll_offset = self.scroll_offset.saturating_sub(rows);
        } else if delta_y > 0 {
            // Scroll down
            let rows = (delta_y as usize / 3).max(1);
            self.scroll_offset = (self.scroll_offset + rows).min(max_scroll);
        }

        self.scroll_offset != old_offset
    }

    /// Start rubber band selection.
    pub fn start_drag(&mut self, pos: Point) {
        self.drag_start = Some(pos);
        self.drag_current = Some(pos);
        self.selected.clear();
        self.last_selection_update = None;
        self.last_drag_render = None;
    }

    /// Check if rubber band drag is active.
    pub fn is_dragging(&self) -> bool {
        self.drag_start.is_some()
    }

    /// Stop rubber band selection.
    pub fn stop_drag(&mut self) {
        // Final selection update before clearing drag state
        self.update_rubber_band_selection();
        self.drag_start = None;
        self.drag_current = None;
        self.last_selection_update = None;
        self.last_drag_render = None;
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
            let visible_count = self.visible_count();
            let start_index = self.scroll_offset * self.columns;
            let end_index = (start_index + self.visible_rows() * self.columns).min(visible_count);

            self.selected.clear();
            for i in start_index..end_index {
                let cell = self.cell_bounds(i);
                if rects_intersect(&band, &cell) {
                    self.selected.insert(i);
                }
            }
        }
    }

    /// Get the entry at the given position (for drag detection).
    pub fn entry_at_point(&self, pos: Point) -> Option<&FileEntry> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        let visible_count = self.visible_count();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + self.visible_rows() * self.columns).min(visible_count);

        for i in start_index..end_index {
            if self.cell_bounds(i).contains_point(pos) {
                return self.visible_entry(i);
            }
        }

        None
    }

    /// Get entry and its bounds at a point.
    pub fn entry_bounds_at_point(&self, pos: Point) -> Option<(&FileEntry, Rect)> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        let visible_count = self.visible_count();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + self.visible_rows() * self.columns).min(visible_count);

        for i in start_index..end_index {
            let bounds = self.cell_bounds(i);
            if bounds.contains_point(pos) {
                if let Some(entry) = self.visible_entry(i) {
                    return Some((entry, bounds));
                }
            }
        }

        None
    }

    /// Handle cell click. Returns index of clicked cell if valid.
    pub fn on_click(&mut self, pos: Point, modifiers: &Modifiers) -> Option<usize> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        let visible_count = self.visible_count();
        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + self.visible_rows() * self.columns).min(visible_count);

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
    pub fn render(&self, renderer: &Renderer, rename_state: Option<&RenameState>) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let visible_count = self.visible_count();
        let visible_rows = self.visible_rows();

        let start_index = self.scroll_offset * self.columns;
        let end_index = (start_index + visible_rows * self.columns).min(visible_count);

        // Pre-compute colors to avoid parsing hex strings in the loop
        let dir_color = Color::new(0.36, 0.62, 0.85, 1.0); // #5c9fd8
        let symlink_color = Color::new(0.78, 0.47, 0.87, 1.0); // #c678dd

        // Pre-compute icon font size
        let icon_font_size = match self.icon_size {
            IconSize::Small => 24.0,
            IconSize::Medium => 32.0,
            IconSize::Large => 48.0,
        };
        let name_font_size = self.icon_size.font_size();
        let icon_size_px = self.icon_size.icon_size();

        for i in start_index..end_index {
            let entry = match self.visible_entry(i) {
                Some(e) => e,
                None => continue,
            };
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
            let is_renaming = rename_state.map_or(false, |s| s.index == i);

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

            // Check for cached thumbnail first (for image and PDF files)
            let mut rendered_thumbnail = false;
            let ext_owned = entry.extension();
            let ext = ext_owned.as_deref();
            if is_supported_image(ext) || is_pdf(ext) {
                if let Some(surface) = self.thumbnail_cache.get(&entry.path) {
                    // Render the thumbnail centered in the icon area
                    let thumb_w = surface.width();
                    let thumb_h = surface.height();
                    let icon_area_size = icon_size_px;
                    let thumb_x = cell.x + (cell.width as i32 - thumb_w as i32) / 2;
                    let thumb_y = cell.y + 8 + (icon_area_size as i32 - thumb_h as i32) / 2;

                    let ctx = renderer.context()?;
                    ctx.set_source_surface(surface.cairo_surface(), thumb_x as f64, thumb_y as f64)?;
                    ctx.paint()?;
                    rendered_thumbnail = true;
                }
                // Thumbnails are requested via request_visible_thumbnails() called before render
            }

            // Fall back to emoji icon if no thumbnail
            if !rendered_thumbnail {
                let icon = match entry.entry_type {
                    EntryType::Directory => "\u{1F4C1}",  // folder emoji
                    EntryType::Symlink => "\u{1F517}",    // link emoji
                    _ => Self::file_icon_for_extension(entry.extension().as_deref()),
                };

                let icon_color = if is_selected {
                    theme.selection_foreground
                } else {
                    match entry.entry_type {
                        EntryType::Directory => dir_color,
                        EntryType::Symlink => symlink_color,
                        _ => theme.item_foreground,
                    }
                };

                let icon_style = TextStyle::new()
                    .font_family(&theme.font_family)
                    .font_size(icon_font_size)
                    .color(icon_color);

                // Center icon horizontally in cell
                let icon_center_x = cell.x + cell.width as i32 / 2;
                let icon_center_y = cell.y + 10 + (icon_font_size / 2.0) as i32;
                renderer.text_centered(icon, Point::new(icon_center_x, icon_center_y), &icon_style)?;
            }

            // Rectangle for the text area below the icon
            let text_rect = Rect::new(
                cell.x + 4,
                cell.y + icon_size_px as i32 + 8,
                cell.width - 8,
                cell.height - icon_size_px - 12,
            );

            if is_renaming {
                // Render rename text field
                if let Some(state) = rename_state {
                    self.render_rename_field(renderer, text_rect, state)?;
                }
            } else {
                // File name (truncated)
                let name_color = if is_selected {
                    theme.selection_foreground
                } else if entry.hidden {
                    theme.item_foreground.with_alpha(0.5)
                } else {
                    theme.item_foreground
                };

                // Use Pango CENTER alignment for proper text centering
                let name_style = TextStyle::new()
                    .font_family(&theme.font_family)
                    .font_size(name_font_size)
                    .color(name_color)
                    .align(TextAlign::Center)
                    .ellipsize(true)
                    .max_width((cell.width - 8) as i32);

                // Render text - avoid allocation for non-symlinks
                if entry.is_symlink {
                    let display_name = format!("{}@", entry.name);
                    renderer.text_in_rect(&display_name, text_rect, &name_style)?;
                } else {
                    renderer.text_in_rect(&entry.name, text_rect, &name_style)?;
                }
            }
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

    /// Render the inline rename text field.
    fn render_rename_field(&self, renderer: &Renderer, rect: Rect, state: &RenameState) -> anyhow::Result<()> {
        let theme = renderer.theme();

        // Background for text field
        let field_rect = Rect::new(rect.x, rect.y, rect.width, 20);
        renderer.fill_rounded_rect(field_rect, 2.0, theme.background)?;
        renderer.stroke_rounded_rect(field_rect, 2.0, theme.selection_background, 1.0)?;

        // Text style
        let text_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(self.icon_size.font_size())
            .color(theme.foreground);

        // Draw the text (centered)
        let text_width = renderer.measure_text(&state.text, &text_style)?.width;
        let text_x = field_rect.x + (field_rect.width as i32 - text_width as i32) / 2;
        let text_y = field_rect.y + 2;
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
