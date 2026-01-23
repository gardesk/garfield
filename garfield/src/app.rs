//! Application state and event loop.

use garfield::ui::pane::SplitDirection;
use garfield::ui::{AddressBar, Breadcrumb, HelpModal, Pane, Sidebar, StatusBar, TabBar, TabInfo, Toolbar, ToolbarAction, ViewMode, TAB_BAR_HEIGHT, TOOLBAR_HEIGHT};
use anyhow::Result;
use gartk_core::{InputEvent, Key, Point, Rect, Theme};
use gartk_render::{Renderer, Surface, TextStyle};
use gartk_x11::{Connection, EventLoop, EventLoopConfig, Window, WindowConfig};
use std::path::PathBuf;
use std::time::Instant;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

/// Height of the breadcrumb bar.
const BREADCRUMB_HEIGHT: u32 = 40;

/// Width of the sidebar.
const SIDEBAR_WIDTH: u32 = 180;

/// Height of the status bar.
const STATUS_BAR_HEIGHT: u32 = 24;

/// Application state.
pub struct App {
    /// X11 window.
    window: Window,
    /// Renderer.
    renderer: Renderer,
    /// Graphics context for blitting.
    gc: u32,
    /// Toolbar with action buttons.
    toolbar: Toolbar,
    /// Breadcrumb path bar.
    breadcrumb: Breadcrumb,
    /// Address bar for path editing.
    address_bar: AddressBar,
    /// Places sidebar.
    sidebar: Sidebar,
    /// Tab bar component.
    tab_bar: TabBar,
    /// Root pane (contains all tabs/splits).
    root_pane: Pane,
    /// Focused pane ID.
    focused_pane_id: u32,
    /// Next pane ID to assign.
    next_pane_id: u32,
    /// Status bar component.
    status_bar: StatusBar,
    /// Help modal overlay.
    help_modal: HelpModal,
    /// Whether the app should quit.
    should_quit: bool,
    /// Pane divider resize in progress (split pane pointer path).
    pane_resize_path: Option<Vec<bool>>,
    /// Sidebar resize in progress.
    sidebar_resizing: bool,
    /// Last click time for double-click detection.
    last_click_time: Option<Instant>,
    /// Last click position for double-click detection.
    last_click_pos: Option<Point>,
    /// Path being dragged for bookmark drop (directory only).
    drag_source_path: Option<PathBuf>,
    /// Name of the item being dragged (for visual feedback).
    drag_label: Option<String>,
    /// Starting position of potential drag.
    drag_start_pos: Option<Point>,
    /// Current mouse position during drag (for visual feedback).
    drag_current_pos: Option<Point>,
    /// Whether drag is actively in progress (moved past threshold).
    drag_active: bool,
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

        let sidebar_w = SIDEBAR_WIDTH;
        let header_height = TAB_BAR_HEIGHT + TOOLBAR_HEIGHT + BREADCRUMB_HEIGHT;

        // Create toolbar (below tab bar)
        let toolbar_bounds = Rect::new(
            sidebar_w as i32,
            TAB_BAR_HEIGHT as i32,
            width - sidebar_w,
            TOOLBAR_HEIGHT,
        );
        let toolbar = Toolbar::new(toolbar_bounds);

        // Create breadcrumb (below toolbar)
        let breadcrumb_bounds = Rect::new(
            sidebar_w as i32,
            (TAB_BAR_HEIGHT + TOOLBAR_HEIGHT) as i32,
            width - sidebar_w,
            BREADCRUMB_HEIGHT,
        );
        let mut breadcrumb = Breadcrumb::new(breadcrumb_bounds);
        breadcrumb.set_path(&current_dir);

        // Create address bar (same bounds as breadcrumb)
        let address_bar = AddressBar::new(breadcrumb_bounds);

        // Create sidebar
        let sidebar_bounds = Rect::new(0, 0, SIDEBAR_WIDTH, height);
        let sidebar = Sidebar::new(sidebar_bounds);

        // Create tab bar
        let tab_bar_bounds = Rect::new(sidebar_w as i32, 0, width - sidebar_w, TAB_BAR_HEIGHT);
        let mut tab_bar = TabBar::new(tab_bar_bounds);

        // Create status bar
        let status_bar_bounds = Rect::new(
            sidebar_w as i32,
            (height - STATUS_BAR_HEIGHT) as i32,
            width - sidebar_w,
            STATUS_BAR_HEIGHT,
        );
        let mut status_bar = StatusBar::new(status_bar_bounds);
        status_bar.set_view_mode("List");

        // Create help modal (full window bounds)
        let help_modal = HelpModal::new(Rect::new(0, 0, width, height));

        // Content area bounds (for panes)
        let content_bounds = Rect::new(
            sidebar_w as i32,
            header_height as i32,
            width - sidebar_w,
            height - header_height - STATUS_BAR_HEIGHT,
        );

        // Create root pane with initial tab
        let root_pane = Pane::new_leaf(current_dir, content_bounds, 1);
        let focused_pane_id = 1;
        let next_pane_id = 2;

        // Initialize tab bar with first tab
        let tabs = vec![TabInfo {
            title: root_pane.active_tab().map(|t| t.title()).unwrap_or_default(),
            active: true,
        }];
        tab_bar.set_tabs(tabs, 0);

        let mut app = Self {
            window,
            renderer,
            gc,
            toolbar,
            breadcrumb,
            address_bar,
            sidebar,
            tab_bar,
            root_pane,
            focused_pane_id,
            next_pane_id,
            status_bar,
            help_modal,
            should_quit: false,
            pane_resize_path: None,
            sidebar_resizing: false,
            last_click_time: None,
            last_click_pos: None,
            drag_source_path: None,
            drag_label: None,
            drag_start_pos: None,
            drag_current_pos: None,
            drag_active: false,
        };

        app.update_status_bar();

        Ok(app)
    }

    /// Get the focused pane.
    fn focused_pane(&self) -> Option<&Pane> {
        self.root_pane.leaf_by_id(self.focused_pane_id)
    }

    /// Get the focused pane (mutable).
    fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.root_pane.leaf_by_id_mut(self.focused_pane_id)
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
                    self.handle_mouse_press(pos, &mouse_event.modifiers);
                    ev.request_redraw();
                }
                InputEvent::MouseRelease(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);
                    self.handle_mouse_release(pos);
                    ev.request_redraw();
                }
                InputEvent::MouseMove(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);
                    self.handle_mouse_move(pos);
                    ev.request_redraw();
                }
                InputEvent::MouseLeave => {
                    self.toolbar.clear_hover();
                    self.breadcrumb.clear_hover();
                    self.sidebar.clear_hover();
                    self.tab_bar.clear_hover();
                    if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            tab.clear_hover();
                        }
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

    /// Handle mouse press.
    fn handle_mouse_press(&mut self, pos: Point, modifiers: &gartk_core::Modifiers) {
        // Check help modal first (clicking outside closes it)
        if self.help_modal.on_click(pos) {
            return;
        }

        // Check tab bar clicks
        if let Some((tab_index, is_close)) = self.tab_bar.on_click(pos) {
            if is_close {
                self.close_tab(tab_index);
            } else {
                self.switch_tab(tab_index);
            }
            return;
        }

        // Check toolbar clicks
        if let Some(action) = self.toolbar.on_click(pos) {
            self.handle_toolbar_action(action);
            return;
        }

        // Check sidebar clicks - try to start bookmark drag first
        if self.sidebar.start_bookmark_drag(pos) {
            // Started potential bookmark drag, don't navigate yet
            return;
        }

        // Check sidebar clicks (for non-bookmark items)
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

        // Check for sidebar resize handle
        if self.sidebar.is_resize_handle(pos) {
            self.sidebar_resizing = true;
            return;
        }

        // Check for pane split divider resize start
        if let Some(path) = self.root_pane.split_divider_at(pos) {
            self.pane_resize_path = Some(path);
            return;
        }

        // Check for column resize start in list view
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(divider) = pane.column_divider_at(pos) {
                pane.start_resize(divider);
                return;
            }
        }

        // Handle pane content clicks (also check for pane focus switch)
        if let Some(leaf) = self.root_pane.leaf_at(pos) {
            if let Some(id) = leaf.id() {
                if id != self.focused_pane_id {
                    self.focused_pane_id = id;
                    self.sync_tab_bar();
                    self.sync_breadcrumb();
                    self.update_status_bar();
                }
            }
        }

        // Check for double-click to enter directory
        let now = Instant::now();
        let is_double_click = if let (Some(last_time), Some(last_pos)) = (self.last_click_time, self.last_click_pos) {
            let elapsed = now.duration_since(last_time);
            let distance = ((pos.x - last_pos.x).pow(2) + (pos.y - last_pos.y).pow(2)) as f64;
            elapsed.as_millis() < 400 && distance.sqrt() < 5.0
        } else {
            false
        };

        // Update click tracking
        self.last_click_time = Some(now);
        self.last_click_pos = Some(pos);

        if is_double_click {
            // Double-click: enter the selected item
            self.enter_selected();
            // Clear click tracking to prevent triple-click
            self.last_click_time = None;
            self.last_click_pos = None;
        } else {
            // Single click: handle selection
            if let Some(pane) = self.focused_pane_mut() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.on_click(pos, modifiers);
                }
            }

            // Capture drag source from entry at click position (for bookmark drag)
            let entry_at_click = self.focused_pane()
                .and_then(|pane| pane.active_tab())
                .and_then(|tab| tab.entry_at_point(pos))
                .filter(|e| e.is_dir())
                .map(|e| (e.path.clone(), e.name.clone()));

            if let Some((path, name)) = entry_at_click {
                self.drag_source_path = Some(path);
                self.drag_label = Some(name);
                self.drag_start_pos = Some(pos);
                self.drag_current_pos = Some(pos);
                self.drag_active = false;
            }
        }
    }

    /// Handle mouse release.
    fn handle_mouse_release(&mut self, pos: Point) {
        // Handle bookmark reorder drag completion
        if self.sidebar.is_bookmark_dragging() {
            self.sidebar.complete_bookmark_drag();
        } else if self.sidebar.bookmark_drag_index().is_some() {
            // Clicked on bookmark but didn't drag - navigate to it
            if let Some(path) = self.sidebar.bookmark_path_at_index() {
                self.navigate_to(path);
            }
            self.sidebar.cancel_bookmark_drag();
        }

        // Handle bookmark drag drop (dragging from file view to sidebar)
        if self.drag_active {
            if let Some(path) = self.drag_source_path.take() {
                if self.sidebar.is_bookmark_drop_zone(pos) {
                    self.sidebar.add_bookmark(&path);
                }
            }
        }

        // Clear drag state
        self.drag_source_path = None;
        self.drag_label = None;
        self.drag_start_pos = None;
        self.drag_current_pos = None;
        self.drag_active = false;
        self.sidebar.set_drop_highlight(false);

        // Clear resize states
        self.pane_resize_path = None;
        self.sidebar_resizing = false;

        if let Some(pane) = self.focused_pane_mut() {
            if pane.is_resizing() {
                pane.stop_resize();
            }
            if pane.is_dragging() {
                pane.stop_drag();
            }
        }
    }

    /// Handle mouse move.
    fn handle_mouse_move(&mut self, pos: Point) {
        // Handle sidebar resize in progress
        if self.sidebar_resizing {
            let new_width = (pos.x - self.sidebar.bounds().x).max(0) as u32;
            self.sidebar.set_width(new_width);
            let size = self.renderer.size();
            self.update_layout(size.width, size.height);
            return;
        }

        // Handle pane divider resize in progress
        if let Some(path) = &self.pane_resize_path {
            let path_clone = path.clone();
            self.root_pane.adjust_split_at(&path_clone, pos);
            return;
        }

        // Handle column resize/drag in progress
        if let Some(pane) = self.focused_pane_mut() {
            if pane.is_resizing() || pane.is_dragging() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.on_mouse_move(pos);
                }
                return;
            }
        }

        // Handle bookmark reorder drag in progress
        if self.sidebar.bookmark_drag_index().is_some() {
            self.sidebar.update_bookmark_drag(pos);
        }

        // Handle bookmark drag in progress (dragging from file view)
        if self.drag_source_path.is_some() {
            // Update current drag position for visual feedback
            self.drag_current_pos = Some(pos);

            // Check if we've moved past the drag threshold (5px)
            if let Some(start_pos) = self.drag_start_pos {
                let distance = ((pos.x - start_pos.x).pow(2) + (pos.y - start_pos.y).pow(2)) as f64;
                if distance.sqrt() > 5.0 {
                    self.drag_active = true;
                }
            }

            // Update sidebar drop highlight if drag is active
            if self.drag_active {
                let is_over_drop_zone = self.sidebar.is_bookmark_drop_zone(pos);
                self.sidebar.set_drop_highlight(is_over_drop_zone);
            }
        }

        self.toolbar.on_mouse_move(pos);
        self.breadcrumb.on_mouse_move(pos);
        self.sidebar.on_mouse_move(pos);
        self.tab_bar.on_mouse_move(pos);

        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.on_mouse_move(pos);
            }
        }
    }

    /// Handle a key press.
    fn handle_key(&mut self, key: &Key, modifiers: &gartk_core::Modifiers) {
        // Handle help modal when visible
        if self.help_modal.is_visible() {
            match key {
                Key::Escape | Key::F1 => self.help_modal.hide(),
                Key::Up | Key::Char('k') => self.help_modal.scroll_up(),
                Key::Down | Key::Char('j') => self.help_modal.scroll_down(),
                _ => {}
            }
            return;
        }

        // F1 toggles help
        if *key == Key::F1 {
            self.help_modal.show();
            return;
        }

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

        // Ctrl+Shift keybinds (splits)
        if modifiers.ctrl && modifiers.shift {
            match key {
                Key::Char('h') | Key::Char('H') => {
                    self.split_horizontal();
                    return;
                }
                Key::Char('v') | Key::Char('V') => {
                    self.split_vertical();
                    return;
                }
                Key::Char('w') | Key::Char('W') => {
                    self.close_pane();
                    return;
                }
                Key::Left => {
                    self.focus_pane_left();
                    return;
                }
                Key::Right => {
                    self.focus_pane_right();
                    return;
                }
                Key::Up => {
                    self.focus_pane_up();
                    return;
                }
                Key::Down => {
                    self.focus_pane_down();
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
                Key::Char('t') | Key::Char('T') => {
                    self.new_tab();
                    return;
                }
                Key::Char('w') | Key::Char('W') => {
                    self.close_active_tab();
                    return;
                }
                Key::Tab => {
                    self.next_tab();
                    return;
                }
                Key::Char('a') | Key::Char('A') => {
                    if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            tab.select_all();
                        }
                    }
                    return;
                }
                Key::Char('b') | Key::Char('B') => {
                    self.sidebar.toggle();
                    let size = self.renderer.size();
                    self.update_layout(size.width, size.height);
                    return;
                }
                Key::Char('d') | Key::Char('D') => {
                    // Bookmark the selected item (must be a directory)
                    let bookmark_path = self.focused_pane()
                        .and_then(|pane| pane.active_tab())
                        .and_then(|tab| tab.selected_entry())
                        .filter(|e| e.is_dir())
                        .map(|e| e.path.clone());

                    if let Some(path) = bookmark_path {
                        self.sidebar.toggle_bookmark(&path);
                    }
                    return;
                }
                Key::Char('l') | Key::Char('L') => {
                    if let Some(pane) = self.focused_pane() {
                        if let Some(tab) = pane.active_tab() {
                            let current = tab.current_path().clone();
                            self.address_bar.activate(&current);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        match key {
            Key::Escape => {
                // Cancel drag first if active
                if self.drag_active || self.drag_source_path.is_some() {
                    self.drag_source_path = None;
                    self.drag_label = None;
                    self.drag_start_pos = None;
                    self.drag_current_pos = None;
                    self.drag_active = false;
                    self.sidebar.set_drop_highlight(false);
                } else if self.address_bar.is_active() {
                    self.address_bar.cancel();
                } else {
                    self.should_quit = true;
                }
            }
            Key::Char('q') => {
                self.should_quit = true;
            }
            Key::Up | Key::Char('k') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.select_prev();
                    }
                }
            }
            Key::Down | Key::Char('j') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.select_next();
                    }
                }
            }
            Key::Home | Key::Char('g') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.select_first();
                    }
                }
            }
            Key::End | Key::Char('G') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.select_last();
                    }
                }
            }
            Key::PageUp => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.page_up();
                    }
                }
            }
            Key::PageDown => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.page_down();
                    }
                }
            }
            Key::Return => {
                self.enter_selected();
            }
            Key::Right | Key::Char('l') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        if tab.view_mode() == ViewMode::Grid {
                            tab.select_right();
                        } else {
                            self.enter_selected();
                            return;
                        }
                    }
                }
            }
            Key::Backspace | Key::Left | Key::Char('h') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        if tab.view_mode() == ViewMode::Grid && *key == Key::Left {
                            tab.select_left();
                        } else {
                            self.go_up();
                            return;
                        }
                    }
                }
            }
            Key::Char('H') => {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.toggle_hidden();
                    }
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

    // === Tab operations ===

    /// Create a new tab in the focused pane.
    fn new_tab(&mut self) {
        let path = if let Some(pane) = self.focused_pane() {
            pane.active_tab().map(|t| t.current_path().clone())
        } else {
            None
        };

        let path = path.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));

        if let Some(pane) = self.focused_pane_mut() {
            pane.add_tab(path);
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Close the active tab in the focused pane.
    fn close_active_tab(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            let should_remove = pane.close_active_tab();
            if should_remove {
                // For now, just quit if last tab is closed
                // TODO: Handle pane removal properly
                self.should_quit = true;
                return;
            }
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Close tab at index.
    fn close_tab(&mut self, index: usize) {
        if let Some(pane) = self.focused_pane_mut() {
            let should_remove = pane.close_tab(index);
            if should_remove {
                self.should_quit = true;
                return;
            }
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Switch to tab at index.
    fn switch_tab(&mut self, index: usize) {
        if let Some(pane) = self.focused_pane_mut() {
            pane.set_active_tab(index);
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Cycle to next tab.
    fn next_tab(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            pane.next_tab();
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    // === Split operations ===

    /// Split the focused pane horizontally.
    fn split_horizontal(&mut self) {
        let path = if let Some(pane) = self.focused_pane() {
            pane.active_tab().map(|t| t.current_path().clone())
        } else {
            None
        };

        let path = path.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));
        let new_id = self.next_pane_id;

        if let Some(pane) = self.focused_pane_mut() {
            if pane.split(SplitDirection::Horizontal, path, new_id).is_some() {
                self.next_pane_id += 1;
                self.focused_pane_id = new_id;
            }
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Split the focused pane vertically.
    fn split_vertical(&mut self) {
        let path = if let Some(pane) = self.focused_pane() {
            pane.active_tab().map(|t| t.current_path().clone())
        } else {
            None
        };

        let path = path.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));
        let new_id = self.next_pane_id;

        if let Some(pane) = self.focused_pane_mut() {
            if pane.split(SplitDirection::Vertical, path, new_id).is_some() {
                self.next_pane_id += 1;
                self.focused_pane_id = new_id;
            }
        }

        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Close the focused pane.
    fn close_pane(&mut self) {
        // Can't close if only one pane
        if self.root_pane.leaf_ids().len() <= 1 {
            return;
        }

        // Remove the focused pane and get sibling to focus
        if let Some(new_focus_id) = self.root_pane.remove_pane(self.focused_pane_id) {
            self.focused_pane_id = new_focus_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.sync_toolbar_view();
            self.update_status_bar();
        }
    }

    /// Focus the pane to the left.
    fn focus_pane_left(&mut self) {
        if let Some(new_id) = self.root_pane.pane_left_of(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.update_status_bar();
        }
    }

    /// Focus the pane to the right.
    fn focus_pane_right(&mut self) {
        if let Some(new_id) = self.root_pane.pane_right_of(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.update_status_bar();
        }
    }

    /// Focus the pane above.
    fn focus_pane_up(&mut self) {
        if let Some(new_id) = self.root_pane.pane_above(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.update_status_bar();
        }
    }

    /// Focus the pane below.
    fn focus_pane_down(&mut self) {
        if let Some(new_id) = self.root_pane.pane_below(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.update_status_bar();
        }
    }

    // === Navigation ===

    /// Enter the selected entry.
    fn enter_selected(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.enter_selected();
            }
        }
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Navigate to a directory.
    fn navigate_to(&mut self, path: PathBuf) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.navigate_to(path);
            }
        }
        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Go back in history.
    fn go_back(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.go_back();
            }
        }
        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Go forward in history.
    fn go_forward(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.go_forward();
            }
        }
        self.sync_tab_bar();
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Go up to parent directory.
    fn go_up(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.go_up();
            }
        }
        self.sync_breadcrumb();
        self.update_status_bar();
    }

    /// Refresh the current directory.
    fn refresh(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.refresh();
            }
        }
        self.update_status_bar();
    }

    /// Set the view mode for the active tab.
    fn set_view_mode(&mut self, mode: ViewMode) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.set_view_mode(mode);
            }
        }
        self.status_bar.set_view_mode(mode.name());
        self.sync_toolbar_view();
        self.update_status_bar();
    }

    /// Handle a toolbar action.
    fn handle_toolbar_action(&mut self, action: ToolbarAction) {
        match action {
            ToolbarAction::ViewList => self.set_view_mode(ViewMode::List),
            ToolbarAction::ViewGrid => self.set_view_mode(ViewMode::Grid),
            ToolbarAction::ViewColumns => self.set_view_mode(ViewMode::Columns),
            ToolbarAction::NewTab => self.new_tab(),
            ToolbarAction::SplitHorizontal => self.split_horizontal(),
            ToolbarAction::SplitVertical => self.split_vertical(),
            ToolbarAction::GoBack => self.go_back(),
            ToolbarAction::GoForward => self.go_forward(),
            ToolbarAction::GoUp => self.go_up(),
            ToolbarAction::Help => self.help_modal.toggle(),
        }
    }

    /// Sync toolbar active view with current tab's view mode.
    fn sync_toolbar_view(&mut self) {
        if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                let action = match tab.view_mode() {
                    ViewMode::List => ToolbarAction::ViewList,
                    ViewMode::Grid => ToolbarAction::ViewGrid,
                    ViewMode::Columns => ToolbarAction::ViewColumns,
                };
                self.toolbar.set_active_view(action);
            }
        }
    }

    // === Sync helpers ===

    /// Sync tab bar with focused pane's tabs.
    fn sync_tab_bar(&mut self) {
        if let Some(pane) = self.focused_pane() {
            let tabs: Vec<TabInfo> = pane
                .tabs()
                .iter()
                .enumerate()
                .map(|(i, t)| TabInfo {
                    title: t.title(),
                    active: i == pane.active_tab_index(),
                })
                .collect();
            self.tab_bar.set_tabs(tabs, pane.active_tab_index());
        }
    }

    /// Sync breadcrumb with active tab's path.
    fn sync_breadcrumb(&mut self) {
        let path = self.focused_pane()
            .and_then(|pane| pane.active_tab())
            .map(|tab| tab.current_path().clone());

        if let Some(path) = path {
            self.breadcrumb.set_path(&path);
        }
    }

    /// Update status bar.
    fn update_status_bar(&mut self) {
        let stats = self.focused_pane()
            .and_then(|pane| pane.active_tab())
            .map(|tab| (tab.visible_count(), tab.selection_count(), tab.selected_size(), tab.view_mode().name()));

        if let Some((visible_count, selected_count, selected_size, view_mode)) = stats {
            self.status_bar.update(visible_count, selected_count, selected_size);
            self.status_bar.set_view_mode(view_mode);
        }
    }

    /// Update layout.
    fn update_layout(&mut self, width: u32, height: u32) {
        // Preserve current sidebar width (or use default if sidebar is hidden)
        let current_sidebar_width = if self.sidebar.is_visible() {
            self.sidebar.bounds().width
        } else {
            SIDEBAR_WIDTH
        };
        let sidebar_w = if self.sidebar.is_visible() { current_sidebar_width } else { 0 };
        let header_height = TAB_BAR_HEIGHT + TOOLBAR_HEIGHT + BREADCRUMB_HEIGHT;

        self.sidebar.set_bounds(Rect::new(0, 0, current_sidebar_width, height));

        self.tab_bar.set_bounds(Rect::new(
            sidebar_w as i32,
            0,
            width - sidebar_w,
            TAB_BAR_HEIGHT,
        ));

        self.toolbar.set_bounds(Rect::new(
            sidebar_w as i32,
            TAB_BAR_HEIGHT as i32,
            width - sidebar_w,
            TOOLBAR_HEIGHT,
        ));

        let breadcrumb_bounds = Rect::new(
            sidebar_w as i32,
            (TAB_BAR_HEIGHT + TOOLBAR_HEIGHT) as i32,
            width - sidebar_w,
            BREADCRUMB_HEIGHT,
        );
        self.breadcrumb.set_bounds(breadcrumb_bounds);
        self.address_bar.set_bounds(breadcrumb_bounds);

        let content_bounds = Rect::new(
            sidebar_w as i32,
            header_height as i32,
            width - sidebar_w,
            height - header_height - STATUS_BAR_HEIGHT,
        );
        self.root_pane.set_bounds(content_bounds);

        self.status_bar.set_bounds(Rect::new(
            sidebar_w as i32,
            (height - STATUS_BAR_HEIGHT) as i32,
            width - sidebar_w,
            STATUS_BAR_HEIGHT,
        ));

        self.help_modal.set_bounds(Rect::new(0, 0, width, height));
    }

    /// Render the application.
    fn render(&mut self) -> Result<()> {
        let theme = self.renderer.theme().clone();
        let size = self.renderer.size();
        let sidebar_w = self.sidebar.width();
        let header_height = TAB_BAR_HEIGHT + TOOLBAR_HEIGHT + BREADCRUMB_HEIGHT;

        // Clear background
        self.renderer.clear()?;

        // Draw sidebar
        self.sidebar.render(&self.renderer)?;

        // Draw tab bar
        self.tab_bar.render(&self.renderer)?;

        // Update and draw toolbar
        let (can_back, can_forward) = if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                (tab.can_go_back(), tab.can_go_forward())
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };
        self.toolbar.set_nav_state(can_back, can_forward);
        self.toolbar.render(&self.renderer)?;

        // Draw breadcrumb or address bar
        if self.address_bar.is_active() {
            self.address_bar.render(&self.renderer)?;
        } else {
            self.breadcrumb.render(&self.renderer, can_back, can_forward)?;
        }

        // Draw separator line under breadcrumb
        self.renderer.line(
            sidebar_w as f64,
            header_height as f64,
            size.width as f64,
            header_height as f64,
            theme.border,
            1.0,
        )?;

        // Draw pane content
        self.root_pane.render(&self.renderer, Some(self.focused_pane_id))?;

        // Draw status bar
        self.status_bar.render(&self.renderer)?;

        // Draw toolbar tooltip overlay (on top of other UI)
        self.toolbar.render_tooltip_overlay(&self.renderer)?;

        // Draw drag label overlay (on top of other UI)
        self.render_drag_label()?;

        // Draw help modal overlay (on top of everything)
        self.help_modal.render(&self.renderer)?;

        // Flush and copy to window
        self.renderer.flush();
        self.blit_surface()?;

        Ok(())
    }

    /// Render the drag label overlay when dragging a folder.
    fn render_drag_label(&self) -> Result<()> {
        // Only render if drag is active and we have a label
        if !self.drag_active {
            return Ok(());
        }

        let (label, pos) = match (&self.drag_label, self.drag_current_pos) {
            (Some(label), Some(pos)) => (label, pos),
            _ => return Ok(()),
        };

        let theme = self.renderer.theme();

        // Create text style for the drag label
        let text_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);

        // Measure the text to size the background
        let text_size = self.renderer.measure_text(label, &text_style)?;

        // Position the label slightly offset from the cursor
        let label_x = pos.x + 16;
        let label_y = pos.y + 8;
        let padding = 8;

        // Draw background with rounded appearance
        let bg_rect = Rect::new(
            label_x - padding,
            label_y - padding / 2,
            text_size.width + (padding * 2) as u32,
            text_size.height + padding as u32,
        );

        // Semi-transparent dark background
        let bg_color = gartk_core::Color::from_u8(40, 40, 45, 230);
        self.renderer.fill_rect(bg_rect, bg_color)?;

        // Border
        let border_color = theme.selection_background.with_alpha(0.8);
        self.renderer.stroke_rect(bg_rect, border_color, 1.0)?;

        // Folder icon prefix
        let icon_style = text_style.clone().color(theme.selection_background);
        self.renderer.text("*", label_x as f64, label_y as f64, &icon_style)?;

        // Draw the text
        self.renderer.text(label, (label_x + 14) as f64, label_y as f64, &text_style)?;

        Ok(())
    }

    /// Blit the rendered surface to the window.
    fn blit_surface(&mut self) -> Result<()> {
        let size = self.renderer.size();
        let conn = self.window.connection();

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
