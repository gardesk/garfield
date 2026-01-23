//! Tab state for a single directory view.

use crate::core::{read_directory, sort_entries, FileEntry, History, SortDirection, SortOrder};
use crate::ui::{ColumnClickResult, ColumnView, GridView, ListView};
use gartk_core::{Modifiers, Point, Rect};
use gartk_render::Renderer;
use std::path::PathBuf;

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

    pub fn on_mouse_move(&mut self, pos: Point) {
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

    // === Rendering ===

    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        match self.view_mode {
            ViewMode::List => self.list_view.render(renderer),
            ViewMode::Grid => self.grid_view.render(renderer),
            ViewMode::Columns => self.column_view.render(renderer),
        }
    }
}
