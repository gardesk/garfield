//! Application state and event loop.

use garfield::core::{read_directory, sort_entries, SortDirection, SortOrder};
use garfield::ui::ListView;
use anyhow::Result;
use gartk_core::{InputEvent, Key, Rect, Theme};
use gartk_render::{Renderer, Surface, TextStyle};
use gartk_x11::{Connection, EventLoop, EventLoopConfig, Window, WindowConfig};
use std::path::PathBuf;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

/// Application state.
pub struct App {
    /// X11 window.
    window: Window,
    /// Renderer.
    renderer: Renderer,
    /// Graphics context for blitting.
    gc: u32,
    /// Current directory path.
    current_dir: PathBuf,
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

        // Create list view
        let list_bounds = Rect::new(0, 40, width, height - 40); // Leave space for path bar
        let mut list_view = ListView::new(list_bounds);

        // Load initial directory
        let mut entries = read_directory(&current_dir).unwrap_or_default();
        sort_entries(&mut entries, SortOrder::Name, SortDirection::Ascending);
        list_view.set_entries(entries);

        Ok(Self {
            window,
            renderer,
            gc,
            current_dir,
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
                    self.handle_key(&key_event.key);
                    ev.request_redraw();
                }
                InputEvent::Resize { width, height } => {
                    let _ = self.renderer.resize(width, height);
                    self.list_view.set_bounds(Rect::new(0, 40, width, height - 40));
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
    fn handle_key(&mut self, key: &Key) {
        match key {
            Key::Escape | Key::Char('q') => {
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
        if let Some(parent) = self.current_dir.parent() {
            self.navigate_to(parent.to_path_buf());
        }
    }

    /// Navigate to a new directory.
    fn navigate_to(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.current_dir = path;
            self.refresh();
        }
    }

    /// Refresh the current directory listing.
    fn refresh(&mut self) {
        let mut entries = read_directory(&self.current_dir).unwrap_or_default();
        sort_entries(&mut entries, self.sort_order, self.sort_direction);
        self.list_view.set_entries(entries);
    }

    /// Render the application.
    fn render(&mut self) -> Result<()> {
        let theme = self.renderer.theme().clone();
        let size = self.renderer.size();

        // Clear background
        self.renderer.clear()?;

        // Draw path bar
        let path_rect = Rect::new(0, 0, size.width, 40);
        self.renderer.fill_rect(path_rect, theme.item_background)?;

        let path_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 2.0)
            .color(theme.item_foreground);

        let path_text = self.current_dir.to_string_lossy();
        let text_rect = Rect::new(16, 0, size.width - 32, 40);
        self.renderer.text_in_rect(&path_text, text_rect, &path_style)?;

        // Draw separator line
        self.renderer.line(0.0, 40.0, size.width as f64, 40.0, theme.border, 1.0)?;

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
