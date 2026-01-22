//! Application state and event loop.

use garfield::core::{read_directory, sort_entries, History, SortDirection, SortOrder};
use garfield::ui::{AddressBar, Breadcrumb, ListView, Sidebar};
use anyhow::Result;
use gartk_core::{InputEvent, Key, Point, Rect, Theme};
use gartk_render::{Renderer, Surface};
use gartk_x11::{Connection, EventLoop, EventLoopConfig, Window, WindowConfig};
use std::path::PathBuf;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

/// Height of the breadcrumb bar.
const BREADCRUMB_HEIGHT: u32 = 40;

/// Width of the sidebar.
const SIDEBAR_WIDTH: u32 = 180;

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

        // Create list view (to the right of sidebar)
        let list_bounds = Rect::new(
            SIDEBAR_WIDTH as i32,
            BREADCRUMB_HEIGHT as i32,
            width - SIDEBAR_WIDTH,
            height - BREADCRUMB_HEIGHT,
        );
        let mut list_view = ListView::new(list_bounds);

        // Load initial directory
        let mut entries = read_directory(&current_dir).unwrap_or_default();
        sort_entries(&mut entries, SortOrder::Name, SortDirection::Ascending);
        list_view.set_entries(entries);

        Ok(Self {
            window,
            renderer,
            gc,
            history,
            breadcrumb,
            address_bar,
            sidebar,
            list_view,
            sort_order: SortOrder::Name,
            sort_direction: SortDirection::Ascending,
            should_quit: false,
        })
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
                    self.handle_click(Point::new(mouse_event.position.x, mouse_event.position.y));
                    ev.request_redraw();
                }
                InputEvent::MouseMove(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);
                    self.breadcrumb.on_mouse_move(pos);
                    self.sidebar.on_mouse_move(pos);
                    ev.request_redraw();
                }
                InputEvent::MouseLeave => {
                    self.breadcrumb.clear_hover();
                    self.sidebar.clear_hover();
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
                self.list_view.select_prev();
            }
            Key::Down | Key::Char('j') => {
                self.list_view.select_next();
            }
            Key::Home | Key::Char('g') => {
                self.list_view.select_first();
            }
            Key::End | Key::Char('G') => {
                self.list_view.select_last();
            }
            Key::PageUp => {
                self.list_view.page_up();
            }
            Key::PageDown => {
                self.list_view.page_down();
            }
            Key::Return | Key::Right | Key::Char('l') => {
                self.enter_selected();
            }
            Key::Backspace | Key::Left | Key::Char('h') => {
                self.go_up();
            }
            Key::Char('H') => {
                self.list_view.toggle_hidden();
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
    fn handle_click(&mut self, pos: Point) {
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

        // TODO: Handle list view clicks
    }

    /// Enter the selected entry (open directory).
    fn enter_selected(&mut self) {
        if let Some(entry) = self.list_view.selected_entry().cloned() {
            if entry.is_dir() {
                self.navigate_to(entry.path);
            }
            // TODO: Open files with default application
        }
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
        self.list_view.set_entries(entries);
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

        self.sidebar.set_bounds(Rect::new(0, 0, SIDEBAR_WIDTH, height));
        self.breadcrumb.set_bounds(bar_bounds);
        self.address_bar.set_bounds(bar_bounds);
        self.list_view.set_bounds(Rect::new(
            sidebar_w as i32,
            BREADCRUMB_HEIGHT as i32,
            width - sidebar_w,
            height - BREADCRUMB_HEIGHT,
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

        // Draw list view
        self.list_view.render(&self.renderer)?;

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
