//! Application state and event loop.

use garfield::core::{read_directory, sort_entries, FileEntry, History, SortDirection, SortOrder};
use garfield::ui::{AddressBar, Breadcrumb, ColumnView, GridView, ListView, Sidebar, StatusBar};
use anyhow::Result;
use gartk_core::{InputEvent, Key, Modifiers, Point, Rect, Theme};
use gartk_render::{Renderer, Surface};
use gartk_x11::{Connection, EventLoop, EventLoopConfig, Window, WindowConfig};
use std::path::PathBuf;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

/// Height of the breadcrumb bar.
const BREADCRUMB_HEIGHT: u32 = 40;

/// Width of the sidebar.
const SIDEBAR_WIDTH: u32 = 180;

/// Height of the status bar.
const STATUS_BAR_HEIGHT: u32 = 24;

/// View mode for the file listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Detailed list view with columns.
    List,
    /// Grid/icon view.
    Grid,
    /// Miller columns view.
    Columns,
}

/// Application state.
pub struct App {
    /// X11 window.
    window: Window,
    /// Renderer.
    renderer: Renderer,
    /// Graphics context for blitting.
    gc: u32,
    /// Navigation history.
    history: History,
    /// Breadcrumb path bar.
    breadcrumb: Breadcrumb,
    /// Address bar for path editing.
    address_bar: AddressBar,
    /// Places sidebar.
    sidebar: Sidebar,
    /// List view component.
    list_view: ListView,
    /// Grid view component.
    grid_view: GridView,
    /// Column view component.
    column_view: ColumnView,
    /// Current view mode.
    view_mode: ViewMode,
    /// Status bar component.
    status_bar: StatusBar,
    /// Sort order.
    sort_order: SortOrder,
    /// Sort direction.
    sort_direction: SortDirection,
    /// Whether the app should quit.
    should_quit: bool,
}

impl App {
    /// Create a new application.
    pub fn new(start_dir: Option<PathBuf>) -> Result<Self> {
        // Connect to X11
        let conn = Connection::connect(None)?;

        // Get primary monitor for window sizing
        let monitor = gartk_x11::primary_monitor(&conn)?;

        // Calculate window size (70% of screen)
        let width = (monitor.rect.width as f64 * 0.7) as u32;
        let height = (monitor.rect.height as f64 * 0.7) as u32;
        let x = monitor.rect.x + (monitor.rect.width as i32 - width as i32) / 2;
        let y = monitor.rect.y + (monitor.rect.height as i32 - height as i32) / 2;

        // Create window
        let window = Window::create(
            conn.clone(),
            WindowConfig::default()
                .title("garfield")
                .class("garfield")
                .position(x, y)
                .size(width, height)
                .transparent(false),
        )?;

        window.focus()?;

        // Create graphics context for blitting
        let gc = conn.generate_id()?;
        conn.inner().create_gc(gc, window.id(), &Default::default())?;
        conn.flush()?;

        // Create renderer with dark theme
        let theme = Theme::dark();
        let renderer = Renderer::with_theme(width, height, theme)?;

        // Determine starting directory
        let current_dir = start_dir
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));

        // Create history
        let history = History::new(current_dir.clone());

        // Create breadcrumb (spans full width, sidebar controls itself)
        let breadcrumb_bounds = Rect::new(SIDEBAR_WIDTH as i32, 0, width - SIDEBAR_WIDTH, BREADCRUMB_HEIGHT);
        let mut breadcrumb = Breadcrumb::new(breadcrumb_bounds);
        breadcrumb.set_path(&current_dir);

        // Create address bar (same bounds as breadcrumb)
        let address_bar = AddressBar::new(breadcrumb_bounds);

        // Create sidebar
        let sidebar_bounds = Rect::new(0, 0, SIDEBAR_WIDTH, height);
        let sidebar = Sidebar::new(sidebar_bounds);

        // Create status bar
        let status_bar_bounds = Rect::new(
            SIDEBAR_WIDTH as i32,
            (height - STATUS_BAR_HEIGHT) as i32,
            width - SIDEBAR_WIDTH,
            STATUS_BAR_HEIGHT,
        );
        let mut status_bar = StatusBar::new(status_bar_bounds);
        status_bar.set_view_mode("List");

        // Content area bounds (for all views)
        let content_bounds = Rect::new(
            SIDEBAR_WIDTH as i32,
            BREADCRUMB_HEIGHT as i32,
            width - SIDEBAR_WIDTH,
            height - BREADCRUMB_HEIGHT - STATUS_BAR_HEIGHT,
        );

        // Create all three views with same bounds
        let mut list_view = ListView::new(content_bounds);
        let mut grid_view = GridView::new(content_bounds);
        let mut column_view = ColumnView::new(content_bounds);

        // Load initial directory
        let mut entries = read_directory(&current_dir).unwrap_or_default();
        sort_entries(&mut entries, SortOrder::Name, SortDirection::Ascending);

        // Initialize all views with entries
        list_view.set_entries(entries.clone());
        grid_view.set_entries(entries.clone());
        column_view.set_entries(entries.clone());
        column_view.set_path(&current_dir, SortOrder::Name, SortDirection::Ascending);

        // Initialize app
        let mut app = Self {
            window,
            renderer,
            gc,
            history,
            breadcrumb,
            address_bar,
            sidebar,
            list_view,
            grid_view,
            column_view,
            view_mode: ViewMode::List,
            status_bar,
            sort_order: SortOrder::Name,
            sort_direction: SortDirection::Ascending,
            should_quit: false,
        };
        app.update_status_bar(&entries);

        Ok(app)
    }

    /// Run the application event loop.
    pub fn run(&mut self) -> Result<()> {
        let mut event_loop = EventLoop::new(&self.window, EventLoopConfig::default())?;

        // Initial render
        self.render()?;

        event_loop.run(|ev, event| {
            match event {
                InputEvent::Key(key_event) if key_event.pressed => {
                    self.handle_key(&key_event.key, &key_event.modifiers);
                    ev.request_redraw();
                }
                InputEvent::MousePress(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);

                    // Check for column resize start in list view
                    if self.view_mode == ViewMode::List {
                        if let Some(divider) = self.list_view.divider_at(pos) {
                            self.list_view.start_resize(divider);
                            ev.request_redraw();
                            return Ok(true);
                        }
                    }

                    self.handle_click(pos, &mouse_event.modifiers);
                    ev.request_redraw();
                }
                InputEvent::MouseRelease(_) => {
                    // Stop column resizing
                    if self.list_view.is_resizing() {
                        self.list_view.stop_resize();
                        ev.request_redraw();
                    }
                    // Stop rubber band selection
                    if self.grid_view.is_dragging() {
                        self.grid_view.stop_drag();
                        ev.request_redraw();
                    }
                }
                InputEvent::MouseMove(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);

                    // During list view column resize, only track the list view
                    if self.list_view.is_resizing() {
                        self.list_view.on_mouse_move(pos);
                        ev.request_redraw();
                        return Ok(!self.should_quit);
                    }

                    // During grid view rubber band, only track the grid view
                    if self.grid_view.is_dragging() {
                        self.grid_view.on_mouse_move(pos);
                        ev.request_redraw();
                        return Ok(!self.should_quit);
                    }

                    self.breadcrumb.on_mouse_move(pos);
                    self.sidebar.on_mouse_move(pos);
                    match self.view_mode {
                        ViewMode::List => self.list_view.on_mouse_move(pos),
                        ViewMode::Grid => self.grid_view.on_mouse_move(pos),
                        ViewMode::Columns => self.column_view.on_mouse_move(pos),
                    }
                    ev.request_redraw();
                }
                InputEvent::MouseLeave => {
                    self.breadcrumb.clear_hover();
                    self.sidebar.clear_hover();
                    match self.view_mode {
                        ViewMode::List => self.list_view.clear_hover(),
                        ViewMode::Grid => self.grid_view.clear_hover(),
                        ViewMode::Columns => self.column_view.clear_hover(),
                    }
                    ev.request_redraw();
                }
                InputEvent::Resize { width, height } => {
                    let _ = self.renderer.resize(width, height);
                    self.update_layout(width, height);
                    ev.request_redraw();
                }
                InputEvent::Expose => {
                    ev.request_redraw();
                }
                InputEvent::CloseRequested => {
                    self.should_quit = true;
                }
                _ => {}
            }

            if ev.needs_redraw() {
                let _ = self.render();
                ev.redraw_done();
            }

            Ok(!self.should_quit)
        })?;

        Ok(())
    }

    /// Handle a key press.
    fn handle_key(&mut self, key: &Key, modifiers: &gartk_core::Modifiers) {
        // Handle address bar input first
        if self.address_bar.is_active() {
            if *key == Key::Return {
                if let Some(path) = self.address_bar.confirm() {
                    self.navigate_to(path);
                }
                return;
            }
            if self.address_bar.handle_key(key) {
                return;
            }
        }

        // Alt+Arrow for history navigation
        if modifiers.alt {
            match key {
                Key::Left => {
                    self.go_back();
                    return;
                }
                Key::Right => {
                    self.go_forward();
                    return;
                }
                _ => {}
            }
        }

        // Ctrl keybinds
        if modifiers.ctrl {
            match key {
                Key::Char('1') => {
                    self.set_view_mode(ViewMode::List);
                    return;
                }
                Key::Char('2') => {
                    self.set_view_mode(ViewMode::Grid);
                    return;
                }
                Key::Char('3') => {
                    self.set_view_mode(ViewMode::Columns);
                    return;
                }
                Key::Char('a') => {
                    // Select all in active view
                    match self.view_mode {
                        ViewMode::List => self.list_view.select_all(),
                        ViewMode::Grid => self.grid_view.select_all(),
                        ViewMode::Columns => self.column_view.select_all(),
                    }
                    return;
                }
                Key::Char('b') => {
                    self.sidebar.toggle();
                    let size = self.renderer.size();
                    self.update_layout(size.width, size.height);
                    return;
                }
                Key::Char('d') => {
                    // Toggle bookmark for current directory
                    let current = self.history.current().clone();
                    self.sidebar.toggle_bookmark(&current);
                    return;
                }
                Key::Char('l') => {
                    // Activate address bar
                    let current = self.history.current().clone();
                    self.address_bar.activate(&current);
                    return;
                }
                _ => {}
            }
        }

        match key {
            Key::Escape => {
                if self.address_bar.is_active() {
                    self.address_bar.cancel();
                } else {
                    self.should_quit = true;
                }
            }
            Key::Char('q') => {
                self.should_quit = true;
            }
            Key::Up | Key::Char('k') => {
                match self.view_mode {
                    ViewMode::List => self.list_view.select_prev(),
                    ViewMode::Grid => self.grid_view.select_prev(),
                    ViewMode::Columns => self.column_view.select_prev(),
                }
            }
            Key::Down | Key::Char('j') => {
                match self.view_mode {
                    ViewMode::List => self.list_view.select_next(),
                    ViewMode::Grid => self.grid_view.select_next(),
                    ViewMode::Columns => self.column_view.select_next(),
                }
            }
            Key::Home | Key::Char('g') => {
                match self.view_mode {
                    ViewMode::List => self.list_view.select_first(),
                    ViewMode::Grid => self.grid_view.select_first(),
                    ViewMode::Columns => self.column_view.select_first(),
                }
            }
            Key::End | Key::Char('G') => {
                match self.view_mode {
                    ViewMode::List => self.list_view.select_last(),
                    ViewMode::Grid => self.grid_view.select_last(),
                    ViewMode::Columns => self.column_view.select_last(),
                }
            }
            Key::PageUp => {
                match self.view_mode {
                    ViewMode::List => self.list_view.page_up(),
                    ViewMode::Grid => self.grid_view.page_up(),
                    ViewMode::Columns => self.column_view.page_up(),
                }
            }
            Key::PageDown => {
                match self.view_mode {
                    ViewMode::List => self.list_view.page_down(),
                    ViewMode::Grid => self.grid_view.page_down(),
                    ViewMode::Columns => self.column_view.page_down(),
                }
            }
            Key::Return | Key::Right | Key::Char('l') => {
                // For grid view, left/right navigate within row
                if self.view_mode == ViewMode::Grid && *key == Key::Right {
                    self.grid_view.select_right();
                } else {
                    self.enter_selected();
                }
            }
            Key::Backspace | Key::Left | Key::Char('h') => {
                // For grid view, left navigates within row
                if self.view_mode == ViewMode::Grid && *key == Key::Left {
                    self.grid_view.select_left();
                } else {
                    self.go_up();
                }
            }
            Key::Char('H') => {
                match self.view_mode {
                    ViewMode::List => self.list_view.toggle_hidden(),
                    ViewMode::Grid => self.grid_view.toggle_hidden(),
                    ViewMode::Columns => self.column_view.toggle_hidden(),
                }
            }
            Key::Char('~') => {
                self.navigate_to(dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));
            }
            Key::Char('/') => {
                self.navigate_to(PathBuf::from("/"));
            }
            Key::Char('r') => {
                self.refresh();
            }
            _ => {}
        }
    }

    /// Handle mouse click.
    fn handle_click(&mut self, pos: Point, modifiers: &gartk_core::Modifiers) {
        // Check sidebar clicks first
        if let Some(path) = self.sidebar.on_click(pos) {
            self.navigate_to(path);
            return;
        }

        // Check breadcrumb back button
        if self.breadcrumb.back_button_bounds().contains_point(pos) {
            self.go_back();
            return;
        }

        // Check breadcrumb forward button
        if self.breadcrumb.forward_button_bounds().contains_point(pos) {
            self.go_forward();
            return;
        }

        // Check breadcrumb segments
        if let Some(path) = self.breadcrumb.on_click(pos) {
            self.navigate_to(path);
            return;
        }

        // Handle view-specific clicks
        match self.view_mode {
            ViewMode::List => {
                // Check list view header click (for sorting)
                if let Some((order, direction)) = self.list_view.on_header_click(pos) {
                    self.sort_order = order;
                    self.sort_direction = direction;
                    self.refresh();
                    return;
                }
                // Check list view row click (with modifiers for multi-select)
                if self.list_view.on_row_click(pos, modifiers).is_some() {
                    return;
                }
            }
            ViewMode::Grid => {
                if self.grid_view.on_click(pos, modifiers).is_some() {
                    return;
                }
            }
            ViewMode::Columns => {
                if self.column_view.on_click(pos, modifiers).is_some() {
                    return;
                }
            }
        }
    }

    /// Enter the selected entry (open directory).
    fn enter_selected(&mut self) {
        let entry = match self.view_mode {
            ViewMode::List => self.list_view.selected_entry().cloned(),
            ViewMode::Grid => self.grid_view.selected_entry().cloned(),
            ViewMode::Columns => self.column_view.selected_entry().cloned(),
        };

        if let Some(entry) = entry {
            if entry.is_dir() {
                self.navigate_to(entry.path);
            }
            // TODO: Open files with default application
        }
    }

    /// Set the current view mode.
    fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode == mode {
            return;
        }

        self.view_mode = mode;
        let mode_name = match mode {
            ViewMode::List => "List",
            ViewMode::Grid => "Grid",
            ViewMode::Columns => "Columns",
        };
        self.status_bar.set_view_mode(mode_name);

        // Sync selection state between views on switch
        // (For now, just refresh to ensure consistency)
        self.refresh();
    }

    /// Navigate to parent directory.
    fn go_up(&mut self) {
        if let Some(parent) = self.history.current().parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    /// Go back in history.
    fn go_back(&mut self) {
        if let Some(path) = self.history.go_back().cloned() {
            self.load_directory(&path);
        }
    }

    /// Go forward in history.
    fn go_forward(&mut self) {
        if let Some(path) = self.history.go_forward().cloned() {
            self.load_directory(&path);
        }
    }

    /// Navigate to a new directory (adds to history).
    fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() && path != *self.history.current() {
            self.history.navigate(path.clone());
            self.load_directory(&path);
        }
    }

    /// Load a directory (without modifying history).
    fn load_directory(&mut self, path: &PathBuf) {
        self.breadcrumb.set_path(path);
        let mut entries = read_directory(path).unwrap_or_default();
        sort_entries(&mut entries, self.sort_order, self.sort_direction);

        // Update all views with entries
        self.list_view.set_entries(entries.clone());
        self.grid_view.set_entries(entries.clone());
        self.column_view.set_entries(entries.clone());
        self.column_view.set_path(path, self.sort_order, self.sort_direction);

        self.update_status_bar(&entries);
    }

    /// Update status bar with current directory info.
    fn update_status_bar(&mut self, entries: &[FileEntry]) {
        let visible_count = entries.iter().filter(|e| !e.hidden).count();
        let (selected_count, selected_size) = match self.view_mode {
            ViewMode::List => {
                let count = self.list_view.selection_count();
                let size: u64 = self.list_view.selected_entries().iter().map(|e| e.size).sum();
                (count, size)
            }
            ViewMode::Grid => {
                let count = self.grid_view.selection_count();
                let size: u64 = self.grid_view.selected_entries().iter().map(|e| e.size).sum();
                (count, size)
            }
            ViewMode::Columns => {
                let count = self.column_view.selection_count();
                let size: u64 = self.column_view.selected_entries().iter().map(|e| e.size).sum();
                (count, size)
            }
        };
        self.status_bar.update(visible_count, selected_count, selected_size);
    }

    /// Refresh the current directory listing.
    fn refresh(&mut self) {
        let path = self.history.current().clone();
        self.load_directory(&path);
    }

    /// Update layout based on sidebar visibility.
    fn update_layout(&mut self, width: u32, height: u32) {
        let sidebar_w = self.sidebar.width();
        let bar_bounds = Rect::new(
            sidebar_w as i32,
            0,
            width - sidebar_w,
            BREADCRUMB_HEIGHT,
        );

        let content_bounds = Rect::new(
            sidebar_w as i32,
            BREADCRUMB_HEIGHT as i32,
            width - sidebar_w,
            height - BREADCRUMB_HEIGHT - STATUS_BAR_HEIGHT,
        );

        self.sidebar.set_bounds(Rect::new(0, 0, SIDEBAR_WIDTH, height));
        self.breadcrumb.set_bounds(bar_bounds);
        self.address_bar.set_bounds(bar_bounds);
        self.list_view.set_bounds(content_bounds);
        self.grid_view.set_bounds(content_bounds);
        self.column_view.set_bounds(content_bounds);
        self.status_bar.set_bounds(Rect::new(
            sidebar_w as i32,
            (height - STATUS_BAR_HEIGHT) as i32,
            width - sidebar_w,
            STATUS_BAR_HEIGHT,
        ));
    }

    /// Render the application.
    fn render(&mut self) -> Result<()> {
        let theme = self.renderer.theme().clone();
        let size = self.renderer.size();
        let sidebar_w = self.sidebar.width();

        // Clear background
        self.renderer.clear()?;

        // Draw sidebar
        self.sidebar.render(&self.renderer)?;

        // Draw breadcrumb or address bar
        if self.address_bar.is_active() {
            self.address_bar.render(&self.renderer)?;
        } else {
            self.breadcrumb.render(
                &self.renderer,
                self.history.can_go_back(),
                self.history.can_go_forward(),
            )?;
        }

        // Draw separator line under breadcrumb
        self.renderer.line(
            sidebar_w as f64,
            BREADCRUMB_HEIGHT as f64,
            size.width as f64,
            BREADCRUMB_HEIGHT as f64,
            theme.border,
            1.0,
        )?;

        // Draw active view
        match self.view_mode {
            ViewMode::List => self.list_view.render(&self.renderer)?,
            ViewMode::Grid => self.grid_view.render(&self.renderer)?,
            ViewMode::Columns => self.column_view.render(&self.renderer)?,
        }

        // Draw status bar
        self.status_bar.render(&self.renderer)?;

        // Flush and copy to window
        self.renderer.flush();
        self.blit_surface()?;

        Ok(())
    }

    /// Blit the rendered surface to the window.
    fn blit_surface(&mut self) -> Result<()> {
        let size = self.renderer.size();
        let conn = self.window.connection();

        // Get the surface data by creating a temp surface and copying
        let ctx = self.renderer.context()?;
        ctx.target().flush();

        let mut temp_surface = Surface::new(size.width, size.height)?;
        let temp_ctx = temp_surface.context()?;
        temp_ctx.set_source_surface(self.renderer.surface().cairo_surface(), 0.0, 0.0)?;
        temp_ctx.paint()?;
        drop(temp_ctx);

        let data = temp_surface.data()?;

        conn.inner().put_image(
            ImageFormat::Z_PIXMAP,
            self.window.id(),
            self.gc,
            size.width as u16,
            size.height as u16,
            0,
            0,
            0,
            self.window.depth(),
            &data,
        )?;

        conn.flush()?;

        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.window.connection().inner().free_gc(self.gc);
    }
}
