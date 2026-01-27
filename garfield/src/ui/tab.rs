//! Tab state for a single directory view.

use crate::core::{read_directory, rename_path, sort_entries, FileEntry, History, SortDirection, SortOrder};
use crate::ui::{ColumnClickResult, ColumnView, GridView, ListView};
use gartk_core::{Key, Modifiers, Point, Rect};
use gartk_render::Renderer;
use std::path::PathBuf;

/// State for inline rename operation.
#[derive(Debug, Clone)]
pub struct RenameState {
    /// Index of the entry being renamed.
    pub index: usize,
    /// Original filename (for cancel).
    pub original: String,
    /// Current edited text.
    pub text: String,
    /// Cursor position in the text.
    pub cursor: usize,
    /// Selection start (for text selection).
    pub selection_start: Option<usize>,
}

/// View mode for displaying files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
    Columns,
}

impl ViewMode {
    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            ViewMode::List => "List",
            ViewMode::Grid => "Grid",
            ViewMode::Columns => "Columns",
        }
    }
}

/// A single tab containing directory state and views.
pub struct Tab {
    /// Navigation history for this tab.
    history: History,
    /// Current view mode.
    view_mode: ViewMode,
    /// List view instance.
    list_view: ListView,
    /// Grid view instance.
    grid_view: GridView,
    /// Column view instance.
    column_view: ColumnView,
    /// Current sort order.
    sort_order: SortOrder,
    /// Current sort direction.
    sort_direction: SortDirection,
    /// Cached entries for the current directory.
    entries: Vec<FileEntry>,
    /// Tab bounds (for rendering).
    bounds: Rect,
    /// Active rename operation (if any).
    renaming: Option<RenameState>,
}

impl Tab {
    /// Create a new tab for the given directory.
    pub fn new(path: PathBuf, bounds: Rect) -> Self {
        let history = History::new(path.clone());

        let mut list_view = ListView::new(bounds);
        let mut grid_view = GridView::new(bounds);
        let mut column_view = ColumnView::new(bounds);

        // Load initial directory
        let mut entries = read_directory(&path).unwrap_or_default();
        sort_entries(&mut entries, SortOrder::Name, SortDirection::Ascending);

        list_view.set_entries(entries.clone());
        grid_view.set_entries(entries.clone());
        column_view.set_entries(entries.clone());
        column_view.set_path(&path, SortOrder::Name, SortDirection::Ascending);

        Self {
            history,
            view_mode: ViewMode::List,
            list_view,
            grid_view,
            column_view,
            sort_order: SortOrder::Name,
            sort_direction: SortDirection::Ascending,
            entries,
            bounds,
            renaming: None,
        }
    }

    /// Get the tab title (directory name).
    pub fn title(&self) -> String {
        self.history
            .current()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string())
    }

    /// Get the current directory path.
    pub fn current_path(&self) -> &PathBuf {
        self.history.current()
    }

    /// Get the current view mode.
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Set the view mode.
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
    }

    /// Cycle icon size in grid view.
    pub fn cycle_icon_size(&mut self) {
        self.grid_view.cycle_icon_size();
    }

    /// Get history reference.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Check if can go back.
    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    /// Check if can go forward.
    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    /// Navigate to a new directory.
    pub fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() && path != *self.history.current() {
            self.history.navigate(path.clone());
            self.load_directory(&path);
        }
    }

    /// Go back in history.
    pub fn go_back(&mut self) {
        if let Some(path) = self.history.go_back().cloned() {
            self.load_directory(&path);
        }
    }

    /// Go forward in history.
    pub fn go_forward(&mut self) {
        if let Some(path) = self.history.go_forward().cloned() {
            self.load_directory(&path);
        }
    }

    /// Go up to parent directory.
    pub fn go_up(&mut self) {
        if let Some(parent) = self.history.current().parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    /// Load a directory without modifying history.
    fn load_directory(&mut self, path: &PathBuf) {
        let mut entries = read_directory(path).unwrap_or_default();
        sort_entries(&mut entries, self.sort_order, self.sort_direction);

        self.entries = entries.clone();
        self.list_view.set_entries(entries.clone());
        self.grid_view.set_entries(entries.clone());
        self.column_view.set_entries(entries.clone());
        self.column_view.set_path(path, self.sort_order, self.sort_direction);
    }

    /// Refresh the current directory.
    pub fn refresh(&mut self) {
        let path = self.history.current().clone();
        self.load_directory(&path);
    }

    /// Get the current entries.
    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// Get visible entry count (excluding hidden if applicable).
    pub fn visible_count(&self) -> usize {
        match self.view_mode {
            ViewMode::List => self.list_view.visible_entries().len(),
            ViewMode::Grid => self.grid_view.visible_entries().len(),
            ViewMode::Columns => self.column_view.visible_entries().len(),
        }
    }

    /// Get selection count.
    pub fn selection_count(&self) -> usize {
        match self.view_mode {
            ViewMode::List => self.list_view.selection_count(),
            ViewMode::Grid => self.grid_view.selection_count(),
            ViewMode::Columns => self.column_view.selection_count(),
        }
    }

    /// Get total size of selected files.
    pub fn selected_size(&self) -> u64 {
        match self.view_mode {
            ViewMode::List => self.list_view.selected_entries().iter().map(|e| e.size).sum(),
            ViewMode::Grid => self.grid_view.selected_entries().iter().map(|e| e.size).sum(),
            ViewMode::Columns => self.column_view.selected_entries().iter().map(|e| e.size).sum(),
        }
    }

    /// Get the selected entry (for enter/open).
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        match self.view_mode {
            ViewMode::List => self.list_view.selected_entry(),
            ViewMode::Grid => self.grid_view.selected_entry(),
            ViewMode::Columns => self.column_view.selected_entry(),
        }
    }

    /// Get paths of all selected entries.
    pub fn selected_paths(&self) -> Vec<std::path::PathBuf> {
        match self.view_mode {
            ViewMode::List => self.list_view.selected_entries().iter().map(|e| e.path.clone()).collect(),
            ViewMode::Grid => self.grid_view.selected_entries().iter().map(|e| e.path.clone()).collect(),
            ViewMode::Columns => self.column_view.selected_entries().iter().map(|e| e.path.clone()).collect(),
        }
    }

    /// Get the entry at the given position (for drag detection).
    pub fn entry_at_point(&self, pos: Point) -> Option<&FileEntry> {
        match self.view_mode {
            ViewMode::List => self.list_view.entry_at_point(pos),
            ViewMode::Grid => self.grid_view.entry_at_point(pos),
            ViewMode::Columns => self.column_view.entry_at_point(pos),
        }
    }

    /// Enter the selected entry (open directory).
    pub fn enter_selected(&mut self) {
        if let Some(entry) = self.selected_entry().cloned() {
            if entry.is_dir() {
                self.navigate_to(entry.path);
            }
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.list_view.set_bounds(bounds);
        self.grid_view.set_bounds(bounds);
        self.column_view.set_bounds(bounds);
    }

    /// Handle sort change from list view header click.
    pub fn set_sort(&mut self, order: SortOrder, direction: SortDirection) {
        self.sort_order = order;
        self.sort_direction = direction;
        self.refresh();
    }

    /// Get current sort order.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order
    }

    /// Get current sort direction.
    pub fn sort_direction(&self) -> SortDirection {
        self.sort_direction
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.clear_selection(),
            ViewMode::Grid => self.grid_view.clear_selection(),
            ViewMode::Columns => self.column_view.clear_selection(),
        }
    }

    // === Navigation (keyboard) ===

    pub fn select_prev(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.select_prev(),
            ViewMode::Grid => self.grid_view.select_prev(),
            ViewMode::Columns => self.column_view.select_prev(),
        }
    }

    pub fn select_next(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.select_next(),
            ViewMode::Grid => self.grid_view.select_next(),
            ViewMode::Columns => self.column_view.select_next(),
        }
    }

    pub fn select_left(&mut self) {
        if self.view_mode == ViewMode::Grid {
            self.grid_view.select_left();
        }
    }

    pub fn select_right(&mut self) {
        if self.view_mode == ViewMode::Grid {
            self.grid_view.select_right();
        }
    }

    pub fn select_first(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.select_first(),
            ViewMode::Grid => self.grid_view.select_first(),
            ViewMode::Columns => self.column_view.select_first(),
        }
    }

    pub fn select_last(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.select_last(),
            ViewMode::Grid => self.grid_view.select_last(),
            ViewMode::Columns => self.column_view.select_last(),
        }
    }

    pub fn page_up(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.page_up(),
            ViewMode::Grid => self.grid_view.page_up(),
            ViewMode::Columns => self.column_view.page_up(),
        }
    }

    pub fn page_down(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.page_down(),
            ViewMode::Grid => self.grid_view.page_down(),
            ViewMode::Columns => self.column_view.page_down(),
        }
    }

    pub fn select_all(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.select_all(),
            ViewMode::Grid => self.grid_view.select_all(),
            ViewMode::Columns => self.column_view.select_all(),
        }
    }

    pub fn toggle_hidden(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.toggle_hidden(),
            ViewMode::Grid => self.grid_view.toggle_hidden(),
            ViewMode::Columns => self.column_view.toggle_hidden(),
        }
    }

    // === Mouse handling ===

    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        match self.view_mode {
            ViewMode::List => self.list_view.on_mouse_move(pos),
            ViewMode::Grid => self.grid_view.on_mouse_move(pos),
            ViewMode::Columns => self.column_view.on_mouse_move(pos),
        }
    }

    pub fn on_click(&mut self, pos: Point, modifiers: &Modifiers) -> bool {
        match self.view_mode {
            ViewMode::List => {
                // Check header click for sorting
                if let Some((order, direction)) = self.list_view.on_header_click(pos) {
                    self.set_sort(order, direction);
                    return true;
                }
                self.list_view.on_row_click(pos, modifiers).is_some()
            }
            ViewMode::Grid => self.grid_view.on_click(pos, modifiers).is_some(),
            ViewMode::Columns => {
                match self.column_view.on_click(pos, modifiers) {
                    ColumnClickResult::Selected(_) => true,
                    ColumnClickResult::Navigate(path) => {
                        self.navigate_to(path);
                        true
                    }
                    ColumnClickResult::None => false,
                }
            }
        }
    }

    pub fn clear_hover(&mut self) {
        match self.view_mode {
            ViewMode::List => self.list_view.clear_hover(),
            ViewMode::Grid => self.grid_view.clear_hover(),
            ViewMode::Columns => self.column_view.clear_hover(),
        }
    }

    // === List view specific (column resize) ===

    pub fn divider_at(&self, pos: Point) -> Option<usize> {
        if self.view_mode == ViewMode::List {
            self.list_view.divider_at(pos)
        } else {
            None
        }
    }

    pub fn start_resize(&mut self, divider: usize) {
        self.list_view.start_resize(divider);
    }

    pub fn stop_resize(&mut self) {
        self.list_view.stop_resize();
    }

    pub fn is_resizing(&self) -> bool {
        self.list_view.is_resizing()
    }

    // === Grid view specific (rubber band) ===

    pub fn is_dragging(&self) -> bool {
        self.grid_view.is_dragging()
    }

    pub fn stop_drag(&mut self) {
        self.grid_view.stop_drag();
    }

    // === Rename operations ===

    /// Check if rename is in progress.
    pub fn is_renaming(&self) -> bool {
        self.renaming.is_some()
    }

    /// Get the current rename state.
    pub fn rename_state(&self) -> Option<&RenameState> {
        self.renaming.as_ref()
    }

    /// Start renaming the selected entry.
    pub fn start_rename(&mut self) {
        let (index, name) = match self.view_mode {
            ViewMode::List => {
                if let Some(entry) = self.list_view.selected_entry() {
                    (self.list_view.focused_index(), entry.name.clone())
                } else {
                    return;
                }
            }
            ViewMode::Grid => {
                if let Some(entry) = self.grid_view.selected_entry() {
                    (self.grid_view.focused_index(), entry.name.clone())
                } else {
                    return;
                }
            }
            ViewMode::Columns => {
                if let Some(entry) = self.column_view.selected_entry() {
                    (self.column_view.focused_index(), entry.name.clone())
                } else {
                    return;
                }
            }
        };

        // Select all text initially (cursor at end, selection from start)
        let len = name.len();
        self.renaming = Some(RenameState {
            index,
            original: name.clone(),
            text: name,
            cursor: len,
            selection_start: Some(0),
        });
    }

    /// Start rename with pre-populated text (for paste conflicts).
    pub fn start_rename_with_text(&mut self, suggested_name: &str) {
        let index = match self.view_mode {
            ViewMode::List => self.list_view.focused_index(),
            ViewMode::Grid => self.grid_view.focused_index(),
            ViewMode::Columns => self.column_view.focused_index(),
        };

        // Get the actual current name from the selected entry
        let original = self.visible_entries()
            .get(index)
            .map(|e| e.name.clone())
            .unwrap_or_default();

        // Select all text initially
        let len = suggested_name.len();
        self.renaming = Some(RenameState {
            index,
            original,
            text: suggested_name.to_string(),
            cursor: len,
            selection_start: Some(0),
        });
    }

    /// Select a file by name. Returns true if found and selected.
    pub fn select_by_name(&mut self, name: &str) -> bool {
        let entries = self.visible_entries();
        // Try exact match first
        let index = entries.iter().position(|e| e.name == name)
            // Then try case-insensitive match
            .or_else(|| entries.iter().position(|e| e.name.eq_ignore_ascii_case(name)))
            // Then try matching the end of the path
            .or_else(|| entries.iter().position(|e| e.path.ends_with(name)));

        if let Some(index) = index {
            match self.view_mode {
                ViewMode::List => self.list_view.set_focused(index),
                ViewMode::Grid => self.grid_view.set_focused(index),
                ViewMode::Columns => self.column_view.set_focused(index),
            }
            true
        } else {
            false
        }
    }

    /// Cancel rename operation.
    pub fn cancel_rename(&mut self) {
        self.renaming = None;
    }

    /// Confirm rename operation. Returns Ok(new_name) on success, Err(message) on failure.
    /// Confirm and perform the rename.
    /// Returns (original_path, new_path, new_name) on success.
    pub fn confirm_rename(&mut self) -> Result<(PathBuf, PathBuf, String), String> {
        let state = match self.renaming.take() {
            Some(s) => s,
            None => return Err("No rename in progress".to_string()),
        };

        let new_name = state.text.trim();

        // Validate: non-empty and different from original
        if new_name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }

        // Get the entry path
        let visible = self.visible_entries();
        let entry = visible.get(state.index).ok_or("Entry not found")?;
        let entry_path = entry.path.clone();

        if new_name == state.original {
            // No change, just cancel silently
            return Ok((entry_path.clone(), entry_path, state.original));
        }

        // Validate: no path separators
        if new_name.contains('/') || new_name.contains('\\') {
            return Err("Name cannot contain path separators".to_string());
        }

        // Perform the rename
        match rename_path(&entry_path, new_name) {
            Ok(new_path) => {
                self.refresh();
                // Select the renamed file
                self.select_by_path(&new_path);
                Ok((entry_path, new_path, new_name.to_string()))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Select a file by its path.
    fn select_by_path(&mut self, path: &std::path::Path) {
        let visible = self.visible_entries();
        for (i, entry) in visible.iter().enumerate() {
            if entry.path == path {
                match self.view_mode {
                    ViewMode::List => {
                        self.list_view.set_focused(i);
                    }
                    ViewMode::Grid => {
                        self.grid_view.set_focused(i);
                    }
                    ViewMode::Columns => {
                        self.column_view.set_focused(i);
                    }
                }
                return;
            }
        }
    }

    /// Handle keyboard input during rename. Returns true if handled.
    pub fn handle_rename_key(&mut self, key: &Key) -> bool {
        let state = match self.renaming.as_mut() {
            Some(s) => s,
            None => return false,
        };

        match key {
            Key::Escape => {
                self.cancel_rename();
                true
            }
            Key::Return => {
                // Will be handled by caller to show result
                true
            }
            Key::Backspace => {
                if state.selection_start.is_some() {
                    // Delete selection
                    state.delete_selection();
                } else if state.cursor > 0 {
                    state.cursor -= 1;
                    state.text.remove(state.cursor);
                }
                true
            }
            Key::Delete => {
                if state.selection_start.is_some() {
                    state.delete_selection();
                } else if state.cursor < state.text.len() {
                    state.text.remove(state.cursor);
                }
                true
            }
            Key::Left => {
                state.selection_start = None;
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
                true
            }
            Key::Right => {
                state.selection_start = None;
                if state.cursor < state.text.len() {
                    state.cursor += 1;
                }
                true
            }
            Key::Home => {
                state.selection_start = None;
                state.cursor = 0;
                true
            }
            Key::End => {
                state.selection_start = None;
                state.cursor = state.text.len();
                true
            }
            Key::Char(c) => {
                // Clear selection first
                if state.selection_start.is_some() {
                    state.delete_selection();
                }
                state.text.insert(state.cursor, *c);
                state.cursor += 1;
                true
            }
            _ => false,
        }
    }

    /// Get visible entries (helper for rename).
    fn visible_entries(&self) -> Vec<&FileEntry> {
        match self.view_mode {
            ViewMode::List => self.list_view.visible_entries(),
            ViewMode::Grid => self.grid_view.visible_entries(),
            ViewMode::Columns => self.column_view.visible_entries(),
        }
    }

    // === Rendering ===

    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        match self.view_mode {
            ViewMode::List => self.list_view.render(renderer, self.renaming.as_ref()),
            ViewMode::Grid => self.grid_view.render(renderer, self.renaming.as_ref()),
            ViewMode::Columns => self.column_view.render(renderer),
        }
    }

    // === Async Preview Support ===

    /// Take pending preview request for column view (if in column mode).
    /// Returns (path, sort_order, sort_direction) if a preview needs loading.
    pub fn take_pending_preview(&mut self) -> Option<(PathBuf, SortOrder, SortDirection)> {
        if self.view_mode == ViewMode::Columns {
            self.column_view.take_pending_preview()
        } else {
            None
        }
    }

    /// Set preview entries for column view.
    pub fn set_preview_entries(&mut self, path: &PathBuf, entries: Vec<FileEntry>) {
        if self.view_mode == ViewMode::Columns {
            self.column_view.set_preview_entries(path, entries);
        }
    }

    /// Check if preview is currently loading.
    pub fn is_preview_loading(&self) -> bool {
        self.view_mode == ViewMode::Columns && self.column_view.is_preview_loading()
    }
}

impl RenameState {
    /// Delete selected text (if any).
    fn delete_selection(&mut self) {
        if let Some(start) = self.selection_start.take() {
            let (from, to) = if start < self.cursor {
                (start, self.cursor)
            } else {
                (self.cursor, start)
            };
            self.text.drain(from..to);
            self.cursor = from;
        }
    }
}
