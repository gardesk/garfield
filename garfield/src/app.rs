//! Application state and event loop.

use crate::PickerConfig;
use garfield::core::{
    Clipboard, ClipboardOperation, DragTarget, FileOperation, FileDragController, ImagePreviewLoader, PdfPreviewLoader, PreviewLoader, UndoStack,
    copy_files, move_files, delete_files, create_directory,
    trash_files, restore_from_trash, matches_any_filter,
};
use garfield::ui::pane::SplitDirection;
use garfield::ui::{AddressBar, AppPickerDialog, AppPickerResult, Breadcrumb, ConfirmDialog, ConflictAction, ConflictDialog, ContextMenu, ContextMenuAction, ContextType, DialogResult, HelpModal, IconSize, InputDialog, InputResult, Pane, PaneToolbarClick, PickerToolbar, PickerToolbarClick, ProgressDialog, Sidebar, StatusBar, TabBar, TabBarClickResult, TabInfo, Toolbar, ToolbarAction, ViewMode, TAB_BAR_HEIGHT, TOOLBAR_HEIGHT, PICKER_TOOLBAR_HEIGHT};
use anyhow::Result;
use gartk_core::{InputEvent, Key, MouseButton, Point, Rect, Theme};
use gartk_render::{Renderer, TextStyle};
use gartk_x11::{ClipboardManager, Connection, EventLoop, EventLoopConfig, Window, WindowConfig};
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
    /// File drag controller for drag-to-move operations.
    file_drag: FileDragController,
    /// Clipboard for file operations.
    clipboard: Clipboard,
    /// Confirmation dialog.
    confirm_dialog: ConfirmDialog,
    /// Conflict resolution dialog.
    conflict_dialog: ConflictDialog,
    /// Progress dialog for long operations.
    progress_dialog: ProgressDialog,
    /// Context menu for right-click actions.
    context_menu: ContextMenu,
    /// Input dialog for text entry.
    input_dialog: InputDialog,
    /// Application picker dialog.
    app_picker: AppPickerDialog,
    /// Path pending "Open With" custom application.
    pending_open_with_path: Option<PathBuf>,
    /// Paths pending delete confirmation.
    pending_delete_paths: Vec<PathBuf>,
    /// Undo/redo stack for file operations.
    undo_stack: UndoStack,
    /// Pending paste operation with conflicts.
    pending_paste: Option<PendingPaste>,
    /// Async preview loader for column view.
    preview_loader: PreviewLoader,
    /// Async image preview loader.
    image_preview_loader: ImagePreviewLoader,
    /// Async PDF preview loader.
    pdf_preview_loader: PdfPreviewLoader,
    /// X11 clipboard manager for system clipboard integration.
    x11_clipboard: ClipboardManager,
    /// Picker mode configuration (None for normal browser).
    picker_config: PickerConfig,
    /// Picker toolbar (only used in picker mode).
    picker_toolbar: Option<PickerToolbar>,
}

/// State for a paste operation with conflicts.
struct PendingPaste {
    /// Files to paste.
    files: Vec<PathBuf>,
    /// Clipboard operation type.
    operation: ClipboardOperation,
    /// Destination directory.
    dest_dir: PathBuf,
    /// Files that conflict (exist in destination).
    conflicts: Vec<PathBuf>,
}

impl App {
    /// Create a new application.
    pub fn new(start_dir: Option<PathBuf>, picker_config: PickerConfig) -> Result<Self> {
        // Connect to X11
        let conn = Connection::connect(None)?;

        // Get primary monitor for window sizing
        let monitor = gartk_x11::primary_monitor(&conn)?;

        // Calculate window size (70% of screen, smaller for picker mode)
        let scale = if picker_config.is_picker() { 0.5 } else { 0.7 };
        let width = (monitor.rect.width as f64 * scale) as u32;
        let height = (monitor.rect.height as f64 * scale) as u32;
        let x = monitor.rect.x + (monitor.rect.width as i32 - width as i32) / 2;
        let y = monitor.rect.y + (monitor.rect.height as i32 - height as i32) / 2;

        // Window title depends on mode
        let title = if picker_config.is_picker() {
            picker_config.title.clone().unwrap_or_else(|| {
                if picker_config.mode.is_directory_mode() {
                    "Select Folder".to_string()
                } else {
                    "Open File".to_string()
                }
            })
        } else {
            "garfield".to_string()
        };

        // Create window - use Dialog type and different class for picker mode
        let window_class = if picker_config.is_picker() {
            "garfield-picker"
        } else {
            "garfield"
        };

        let mut window_config = WindowConfig::default()
            .title(&title)
            .class(window_class)
            .position(x, y)
            .size(width, height)
            .transparent(false);

        // Use Dialog window type for picker mode (better focus handling)
        if picker_config.is_picker() {
            window_config = window_config.window_type(gartk_x11::WindowType::Dialog);
        }

        let window = Window::create(conn.clone(), window_config)?;
        conn.flush()?;

        // Create X11 clipboard manager for system clipboard integration
        let x11_clipboard = ClipboardManager::new(conn.clone(), window.id())?;

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

        // Create picker toolbar at BOTTOM of window (only if in picker mode)
        let picker_toolbar = if picker_config.is_picker() {
            let picker_toolbar_bounds = Rect::new(
                sidebar_w as i32,
                (height - PICKER_TOOLBAR_HEIGHT) as i32,
                width - sidebar_w,
                PICKER_TOOLBAR_HEIGHT,
            );
            let mut pt = PickerToolbar::new(picker_toolbar_bounds, picker_config.accept_label.clone());
            // Set filter description if we have filters
            let filters = picker_config.mode.filters();
            if !filters.is_empty() {
                let desc = format!("Filter: {}", filters.join(", "));
                pt.set_filter_description(Some(desc));
            }
            Some(pt)
        } else {
            None
        };

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

        // Create confirm dialog (full window bounds)
        let confirm_dialog = ConfirmDialog::new(Rect::new(0, 0, width, height));

        // Create conflict dialog (full window bounds)
        let conflict_dialog = ConflictDialog::new(Rect::new(0, 0, width, height));

        // Create progress dialog (full window bounds)
        let progress_dialog = ProgressDialog::new(Rect::new(0, 0, width, height));

        // Create context menu (full window bounds for positioning)
        let context_menu = ContextMenu::new(Rect::new(0, 0, width, height));

        // Create input dialog (full window bounds)
        let input_dialog = InputDialog::new(Rect::new(0, 0, width, height));

        // Create app picker dialog (full window bounds)
        let app_picker = AppPickerDialog::new(Rect::new(0, 0, width, height));

        // Content area bounds (for panes)
        // In picker mode, picker toolbar replaces status bar at bottom
        let footer_height = if picker_config.is_picker() {
            PICKER_TOOLBAR_HEIGHT
        } else {
            STATUS_BAR_HEIGHT
        };
        let content_bounds = Rect::new(
            sidebar_w as i32,
            header_height as i32,
            width - sidebar_w,
            height - header_height - footer_height,
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
            file_drag: FileDragController::new(),
            clipboard: Clipboard::new(),
            confirm_dialog,
            conflict_dialog,
            progress_dialog,
            context_menu,
            input_dialog,
            app_picker,
            pending_open_with_path: None,
            pending_delete_paths: Vec::new(),
            undo_stack: UndoStack::new(),
            pending_paste: None,
            preview_loader: PreviewLoader::new(),
            image_preview_loader: ImagePreviewLoader::new(),
            pdf_preview_loader: PdfPreviewLoader::new(),
            x11_clipboard,
            picker_config,
            picker_toolbar,
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

    /// Check if the current selection is valid for picker mode.
    fn has_valid_picker_selection(&self) -> bool {
        let Some(pane) = self.focused_pane() else {
            return false;
        };
        let Some(tab) = pane.active_tab() else {
            return false;
        };

        let selected = tab.selected_entries();
        let filters = self.picker_config.mode.filters();

        // In directory mode, we can always accept (use current directory if nothing selected)
        if self.picker_config.mode.is_directory_mode() {
            // If nothing selected, the current directory is the selection
            if selected.is_empty() {
                return true;
            }
            // Otherwise, at least one directory must be selected
            return selected.iter().any(|e| e.is_dir());
        }

        // In file mode, we need at least one file selected that matches filters
        // If multiple is disabled, we need exactly one matching file
        let matching_file_count = selected.iter()
            .filter(|e| !e.is_dir() && matches_any_filter(e, filters))
            .count();

        if matching_file_count == 0 {
            return false;
        }

        if !self.picker_config.mode.allows_multiple() && matching_file_count > 1 {
            return false;
        }

        true
    }

    /// Get the selected paths for picker mode.
    fn get_picker_selection(&self) -> Vec<PathBuf> {
        let Some(pane) = self.focused_pane() else {
            return Vec::new();
        };
        let Some(tab) = pane.active_tab() else {
            return Vec::new();
        };

        let filters = self.picker_config.mode.filters();

        if self.picker_config.mode.is_directory_mode() {
            // In directory mode, return selected directories or current directory
            let dirs: Vec<_> = tab.selected_entries().iter()
                .filter(|e| e.is_dir())
                .map(|e| e.path.clone())
                .collect();

            if dirs.is_empty() {
                // Return current directory
                vec![tab.current_path().to_path_buf()]
            } else {
                dirs
            }
        } else {
            // In file mode, return selected files that match filters
            tab.selected_entries().iter()
                .filter(|e| !e.is_dir() && matches_any_filter(e, filters))
                .map(|e| e.path.clone())
                .collect()
        }
    }

    /// Output picker selection and exit.
    fn accept_picker_selection(&mut self) {
        let paths = self.get_picker_selection();

        // Output paths to stdout (one per line)
        for path in &paths {
            println!("{}", path.display());
        }

        self.should_quit = true;
    }

    /// Cancel picker and exit with no output.
    fn cancel_picker(&mut self) {
        // Exit with code 1 to indicate cancellation
        self.should_quit = true;
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
                    self.handle_mouse_press(pos, &mouse_event.modifiers, mouse_event.button);
                    ev.request_redraw();
                }
                InputEvent::MouseRelease(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);
                    self.handle_mouse_release(pos);
                    ev.request_redraw();
                }
                InputEvent::MouseMove(mouse_event) => {
                    let pos = Point::new(mouse_event.position.x, mouse_event.position.y);
                    if self.handle_mouse_move(pos) {
                        ev.request_redraw();
                    }
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
                InputEvent::Scroll(scroll_event) => {
                    let pos = Point::new(scroll_event.position.x, scroll_event.position.y);
                    if self.handle_scroll(pos, scroll_event.delta_x, scroll_event.delta_y) {
                        ev.request_redraw();
                    }
                }
                InputEvent::SelectionRequest(req) => {
                    // Another application is requesting our clipboard data
                    tracing::debug!("Received SelectionRequest event");
                    if let Err(e) = self.x11_clipboard.handle_selection_request(&req) {
                        tracing::warn!("Failed to handle selection request: {}", e);
                    }
                }
                InputEvent::SelectionClear => {
                    // We lost clipboard ownership to another application
                    tracing::debug!("Received SelectionClear event");
                    self.x11_clipboard.handle_selection_clear();
                }
                _ => {}
            }

            // Poll for completed async preview loads (directories)
            if let Some(result) = self.preview_loader.poll() {
                if let Some(entries) = result.entries {
                    if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            tab.set_preview_entries(&result.path, entries);
                        }
                    }
                }
                ev.request_redraw();
            }

            // Poll for completed async image preview loads
            if let Some(result) = self.image_preview_loader.poll() {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.set_image_preview(&result.path, result.image);
                    }
                }
                ev.request_redraw();
            }

            // Poll for completed async PDF preview loads
            if let Some(result) = self.pdf_preview_loader.poll() {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.set_pdf_preview(&result.path, result.image);
                    }
                }
                ev.request_redraw();
            }

            // Poll for completed grid view thumbnails
            if let Some(pane) = self.focused_pane_mut() {
                if let Some(tab) = pane.active_tab_mut() {
                    if tab.poll_thumbnails() {
                        ev.request_redraw();
                    }
                }
            }

            // Update file drag flash animation
            if self.file_drag.is_hovering() {
                let flash_complete = self.file_drag.update_flash();
                ev.request_redraw();

                if flash_complete {
                    // Flash sequence complete - trigger auto-enter
                    if let Some(target) = self.file_drag.get_auto_enter_target() {
                        self.handle_file_drag_auto_enter(target);
                    }
                }
            }

            // Check for pending preview requests and submit them
            self.process_pending_previews();
            self.process_pending_image_previews();
            self.process_pending_pdf_previews();

            // Request thumbnails for visible grid items
            if let Some(pane) = self.focused_pane_mut() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.request_visible_thumbnails();
                }
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
    fn handle_mouse_press(&mut self, pos: Point, modifiers: &gartk_core::Modifiers, button: Option<MouseButton>) {
        // Check progress dialog first (blocks all other input)
        if self.progress_dialog.is_visible() {
            self.progress_dialog.on_click(pos);
            return;
        }

        // Check confirm dialog first
        if self.confirm_dialog.is_visible() {
            if let Some(result) = self.confirm_dialog.on_click(pos) {
                self.handle_dialog_result(result);
            }
            return;
        }

        // Check conflict dialog
        if self.conflict_dialog.is_visible() {
            if let Some(action) = self.conflict_dialog.on_click(pos) {
                self.handle_conflict_action(action);
            }
            return;
        }

        // Check input dialog
        if self.input_dialog.is_visible() {
            if let Some(result) = self.input_dialog.on_click(pos) {
                self.handle_input_result(result);
            }
            return;
        }

        // Check app picker
        if self.app_picker.is_visible() {
            if let Some(result) = self.app_picker.on_click(pos) {
                self.handle_app_picker_result(result);
            }
            return;
        }

        // Check help modal (clicking outside closes it)
        if self.help_modal.on_click(pos) {
            return;
        }

        // Check context menu
        if self.context_menu.is_visible() {
            if let Some(action) = self.context_menu.on_click(pos) {
                self.handle_context_menu_action(action);
            }
            return;
        }

        // Right-click shows context menu
        if button == Some(MouseButton::Right) {
            self.show_context_menu(pos);
            return;
        }

        // Handle middle-click on tab bar to close tab
        if button == Some(MouseButton::Middle) {
            if let Some(index) = self.tab_bar.tab_at_point(pos) {
                self.close_tab(index);
                return;
            }
        }

        // Check tab bar clicks - try to start tab drag first (left click only)
        if button == Some(MouseButton::Left) || button.is_none() {
            if self.tab_bar.start_drag(pos) {
                // Started potential tab drag, don't switch yet
                return;
            }

            // Handle tab bar clicks (non-drag)
            match self.tab_bar.on_click(pos) {
                TabBarClickResult::Tab(tab_index, is_close) => {
                    if is_close {
                        self.close_tab(tab_index);
                    }
                    // Tab selection happens on mouse release if not dragged
                    return;
                }
                TabBarClickResult::NewTab => {
                    self.new_tab();
                    return;
                }
                TabBarClickResult::None => {}
            }
        }

        // Check picker toolbar clicks (if in picker mode)
        if let Some(picker_toolbar) = &self.picker_toolbar {
            match picker_toolbar.on_click(pos) {
                PickerToolbarClick::Accept => {
                    self.accept_picker_selection();
                    return;
                }
                PickerToolbarClick::Cancel => {
                    self.cancel_picker();
                    return;
                }
                PickerToolbarClick::None => {}
            }
        } else {
            // Check normal toolbar clicks
            if let Some(action) = self.toolbar.on_click(pos) {
                self.handle_toolbar_action(action);
                return;
            }
        }

        // Check pane toolbar clicks (view mode buttons)
        if let PaneToolbarClick::ViewMode(_mode) = self.root_pane.on_toolbar_click(pos) {
            self.sync_toolbar_view();
            self.update_status_bar();
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

        // Check for pane split divider - double-click to equalize, single-click to resize
        if let Some(path) = self.root_pane.split_divider_at(pos) {
            let now = Instant::now();
            let is_double_click = if let (Some(last_time), Some(last_pos)) = (self.last_click_time, self.last_click_pos) {
                let elapsed = now.duration_since(last_time);
                let distance = ((pos.x - last_pos.x).pow(2) + (pos.y - last_pos.y).pow(2)) as f64;
                elapsed.as_millis() < 400 && distance.sqrt() < 10.0
            } else {
                false
            };

            if is_double_click {
                // Double-click: equalize the split
                self.root_pane.equalize_split_at(&path);
                self.last_click_time = None;
                self.last_click_pos = None;
            } else {
                // Single click: start resize
                self.pane_resize_path = Some(path);
                self.last_click_time = Some(now);
                self.last_click_pos = Some(pos);
            }
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
            // Check if clicking on an already-selected item - start pending file drag
            let start_file_drag = self.focused_pane()
                .and_then(|pane| pane.active_tab())
                .and_then(|tab| {
                    tab.entry_at_point(pos).map(|entry| {
                        let is_selected = tab.is_path_selected(&entry.path);
                        (is_selected, tab.selected_paths())
                    })
                });

            if let Some((true, selected_paths)) = start_file_drag {
                // Clicking on an already-selected item - start pending file drag
                if !selected_paths.is_empty() {
                    self.file_drag.start_pending(pos, selected_paths);
                    self.update_status_bar();
                    return;
                }
            }

            // Single click: handle selection
            if let Some(pane) = self.focused_pane_mut() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.on_click(pos, modifiers);
                }
            }
            // Update status bar with new selection
            self.update_status_bar();

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
        // Handle tab reorder drag completion
        if self.tab_bar.is_dragging() {
            if let Some((from, to)) = self.tab_bar.complete_drag() {
                self.reorder_tab(from, to);
            }
        } else if self.tab_bar.dragging_tab().is_some() {
            // Clicked on tab but didn't drag - select it
            if let Some(index) = self.tab_bar.dragging_tab() {
                self.switch_tab(index);
            }
            self.tab_bar.cancel_drag();
        }

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

        // Handle file drag drop completion
        if self.file_drag.is_dragging() {
            // Get paths BEFORE complete() since it calls cancel() internally
            let dragged_paths = self.file_drag.dragged_paths().cloned();

            if let Some((paths, target)) = self.file_drag.complete() {
                // Dropped on a specific target (hovering state)
                let dest_dir = target.path().clone();
                self.move_files_to_directory(paths, dest_dir);
            } else if let Some(paths) = dragged_paths {
                // Dropped while dragging (not hovering on target) - move to current directory
                // This happens after auto-entering directories via hover
                if let Some(dest_dir) = self.focused_pane()
                    .and_then(|p| p.active_tab())
                    .map(|t| t.current_path().clone())
                {
                    self.move_files_to_directory(paths, dest_dir);
                }
            }
        } else {
            // Cancel file drag if it was pending but didn't activate
            self.file_drag.cancel();
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

        let was_dragging = self.focused_pane().map(|p| p.is_dragging()).unwrap_or(false);

        if let Some(pane) = self.focused_pane_mut() {
            if pane.is_resizing() {
                pane.stop_resize();
            }
            if pane.is_dragging() {
                pane.stop_drag();
            }
        }

        // Update status bar after any mouse release (ensures rubber band selection is reflected)
        if was_dragging {
            self.update_status_bar();
        }
    }

    /// Handle mouse move. Returns true if a redraw is needed.
    fn handle_mouse_move(&mut self, pos: Point) -> bool {
        // Handle progress dialog hover
        if self.progress_dialog.is_visible() {
            self.progress_dialog.on_mouse_move(pos);
            return true; // Dialogs always redraw for responsiveness
        }

        // Handle confirm dialog hover
        if self.confirm_dialog.is_visible() {
            self.confirm_dialog.on_mouse_move(pos);
            return true;
        }

        // Handle conflict dialog hover
        if self.conflict_dialog.is_visible() {
            self.conflict_dialog.on_mouse_move(pos);
            return true;
        }

        // Handle input dialog hover
        if self.input_dialog.is_visible() {
            self.input_dialog.on_mouse_move(pos);
            return true;
        }

        // Handle app picker hover
        if self.app_picker.is_visible() {
            return self.app_picker.on_mouse_move(pos);
        }

        // Handle context menu hover
        if self.context_menu.is_visible() {
            return self.context_menu.on_mouse_move(pos);
        }

        // Handle sidebar resize in progress
        if self.sidebar_resizing {
            let new_width = (pos.x - self.sidebar.bounds().x).max(0) as u32;
            self.sidebar.set_width(new_width);
            let size = self.renderer.size();
            self.update_layout(size.width, size.height);
            return true;
        }

        // Handle pane divider resize in progress
        if let Some(path) = &self.pane_resize_path {
            let path_clone = path.clone();
            self.root_pane.adjust_split_at(&path_clone, pos);
            return true;
        }

        // Handle column resize/drag in progress
        if let Some(pane) = self.focused_pane_mut() {
            if pane.is_resizing() || pane.is_dragging() {
                if let Some(tab) = pane.active_tab_mut() {
                    tab.on_mouse_move(pos);
                }
                return true;
            }
        }

        let mut needs_redraw = false;

        // Handle file drag in progress
        if self.file_drag.is_active() {
            self.file_drag.update_position(pos);

            // If actively dragging (past threshold), detect hover targets and always redraw
            if self.file_drag.is_dragging() {
                needs_redraw = true;  // Always redraw when dragging for smooth cursor tracking
                let target = self.detect_file_drag_target(pos);
                self.file_drag.set_hover_target(target);
            }
        }

        // Handle tab reorder drag in progress
        if self.tab_bar.dragging_tab().is_some() {
            self.tab_bar.update_drag(pos);
            needs_redraw = true;
        }

        // Handle bookmark reorder drag in progress
        if self.sidebar.bookmark_drag_index().is_some() {
            self.sidebar.update_bookmark_drag(pos);
            needs_redraw = true;
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
            needs_redraw = true;
        }

        // Check hover states - only redraw if any changed
        if let Some(picker_toolbar) = &mut self.picker_toolbar {
            needs_redraw |= picker_toolbar.on_mouse_move(pos);
        } else {
            needs_redraw |= self.toolbar.on_mouse_move(pos);
        }
        needs_redraw |= self.breadcrumb.on_mouse_move(pos);
        needs_redraw |= self.sidebar.on_mouse_move(pos);
        needs_redraw |= self.tab_bar.on_mouse_move(pos);
        needs_redraw |= self.root_pane.on_toolbar_mouse_move(pos);

        let mut is_dragging = false;
        let mut selection_count = 0;
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                needs_redraw |= tab.on_mouse_move(pos);
                is_dragging = tab.is_dragging();
                if is_dragging {
                    selection_count = tab.selection_count();
                }
            }
        }

        // Update status bar selection count if rubber band is active (lightweight update)
        if is_dragging {
            self.status_bar.update_selection_count(selection_count);
        }

        needs_redraw
    }

    /// Handle mouse scroll. Returns true if a redraw is needed.
    fn handle_scroll(&mut self, pos: Point, _delta_x: i32, delta_y: i32) -> bool {
        // Help modal captures all scroll events when visible
        if self.help_modal.is_visible() {
            self.help_modal.on_scroll(delta_y);
            return true;
        }

        // Check if scroll is over the content area (not sidebar, toolbar, etc.)
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                if tab.bounds().contains_point(pos) {
                    return tab.on_scroll(delta_y);
                }
            }
        }

        // Check if scroll is over sidebar
        if self.sidebar.bounds().contains_point(pos) {
            return self.sidebar.on_scroll(delta_y);
        }

        false
    }

    /// Handle a key press.
    fn handle_key(&mut self, key: &Key, modifiers: &gartk_core::Modifiers) {
        // Handle progress dialog when visible (blocks all other input)
        if self.progress_dialog.is_visible() {
            self.progress_dialog.handle_key(key);
            return;
        }

        // Handle confirm dialog when visible
        if self.confirm_dialog.is_visible() {
            if let Some(result) = self.confirm_dialog.handle_key(key) {
                self.handle_dialog_result(result);
            }
            return;
        }

        // Handle conflict dialog when visible
        if self.conflict_dialog.is_visible() {
            if let Some(action) = self.conflict_dialog.handle_key(key) {
                self.handle_conflict_action(action);
            }
            return;
        }

        // Handle input dialog when visible
        if self.input_dialog.is_visible() {
            if let Some(result) = self.input_dialog.handle_key(key) {
                self.handle_input_result(result);
            }
            return;
        }

        // Handle app picker when visible
        if self.app_picker.is_visible() {
            if let Some(result) = self.app_picker.handle_key(key) {
                self.handle_app_picker_result(result);
            }
            return;
        }

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

        // Handle context menu when visible
        if self.context_menu.is_visible() {
            if let Some(action) = self.context_menu.handle_key(key) {
                self.handle_context_menu_action(action);
            }
            return;
        }

        // Cancel file drag on Escape
        if *key == Key::Escape && self.file_drag.is_active() {
            self.file_drag.cancel();
            return;
        }

        // Handle Escape in picker mode (cancel)
        if *key == Key::Escape && self.picker_config.is_picker() {
            self.cancel_picker();
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

        // Handle rename input
        if self.is_renaming() {
            match key {
                Key::Escape => {
                    self.cancel_rename();
                    return;
                }
                Key::Return => {
                    self.confirm_rename();
                    return;
                }
                _ => {
                    // Route other keys to rename handler
                    if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            if tab.handle_rename_key(key) {
                                return;
                            }
                        }
                    }
                }
            }
            return;
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
                // Alt+1-9 to jump to tab N
                Key::Char('1') => { self.switch_tab(0); return; }
                Key::Char('2') => { self.switch_tab(1); return; }
                Key::Char('3') => { self.switch_tab(2); return; }
                Key::Char('4') => { self.switch_tab(3); return; }
                Key::Char('5') => { self.switch_tab(4); return; }
                Key::Char('6') => { self.switch_tab(5); return; }
                Key::Char('7') => { self.switch_tab(6); return; }
                Key::Char('8') => { self.switch_tab(7); return; }
                Key::Char('9') => { self.switch_tab(8); return; }
                _ => {}
            }
        }

        // Ctrl+Shift keybinds (splits, new folder, new file, duplicate)
        if modifiers.ctrl && modifiers.shift {
            match key {
                Key::Char('n') | Key::Char('N') => {
                    self.create_new_folder();
                    return;
                }
                Key::Char('f') | Key::Char('F') => {
                    self.create_new_file();
                    return;
                }
                Key::Char('d') | Key::Char('D') => {
                    self.duplicate_selected();
                    return;
                }
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
                Key::Char('+') | Key::Char('=') => {
                    // Increase icon size in grid view
                    if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            tab.increase_icon_size();
                        }
                    }
                    self.sync_toolbar_icon_size();
                    return;
                }
                Key::Char('-') | Key::Char('_') => {
                    // Decrease icon size in grid view
                    if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            tab.decrease_icon_size();
                        }
                    }
                    self.sync_toolbar_icon_size();
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
                Key::Char('c') | Key::Char('C') => {
                    self.copy_selected();
                    return;
                }
                Key::Char('x') | Key::Char('X') => {
                    self.cut_selected();
                    return;
                }
                Key::Char('v') | Key::Char('V') => {
                    self.paste();
                    return;
                }
                Key::Char('z') | Key::Char('Z') => {
                    self.undo();
                    return;
                }
                Key::Char('y') | Key::Char('Y') => {
                    self.redo();
                    return;
                }
                _ => {}
            }
        }

        // Shift+Delete for permanent delete
        if modifiers.shift && *key == Key::Delete {
            self.delete_selected_permanently();
            return;
        }

        match key {
            Key::Delete => {
                self.trash_selected();
            }
            Key::F2 => {
                self.start_rename();
            }
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
        let path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));

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
        self.sync_toolbar_view();
        self.sync_toolbar_icon_size();
        self.update_status_bar();
    }

    /// Reorder a tab from one position to another.
    fn reorder_tab(&mut self, from: usize, to: usize) {
        if let Some(pane) = self.focused_pane_mut() {
            pane.reorder_tab(from, to);
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
        let (path, view_mode) = if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                (Some(tab.current_path().clone()), Some(tab.view_mode()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let path = path.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));
        let new_id = self.next_pane_id;

        if let Some(pane) = self.focused_pane_mut() {
            if pane.split(SplitDirection::Horizontal, path, new_id, view_mode).is_some() {
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
        let (path, view_mode) = if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                (Some(tab.current_path().clone()), Some(tab.view_mode()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let path = path.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")));
        let new_id = self.next_pane_id;

        if let Some(pane) = self.focused_pane_mut() {
            if pane.split(SplitDirection::Vertical, path, new_id, view_mode).is_some() {
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
            self.sync_toolbar_icon_size();
            self.update_status_bar();
        }
    }

    /// Focus the pane to the left.
    fn focus_pane_left(&mut self) {
        if let Some(new_id) = self.root_pane.pane_left_of(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.sync_toolbar_view();
            self.sync_toolbar_icon_size();
            self.update_status_bar();
        }
    }

    /// Focus the pane to the right.
    fn focus_pane_right(&mut self) {
        if let Some(new_id) = self.root_pane.pane_right_of(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.sync_toolbar_view();
            self.sync_toolbar_icon_size();
            self.update_status_bar();
        }
    }

    /// Focus the pane above.
    fn focus_pane_up(&mut self) {
        if let Some(new_id) = self.root_pane.pane_above(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.sync_toolbar_view();
            self.sync_toolbar_icon_size();
            self.update_status_bar();
        }
    }

    /// Focus the pane below.
    fn focus_pane_down(&mut self) {
        if let Some(new_id) = self.root_pane.pane_below(self.focused_pane_id) {
            self.focused_pane_id = new_id;
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.sync_toolbar_view();
            self.sync_toolbar_icon_size();
            self.update_status_bar();
        }
    }

    // === Navigation ===

    /// Enter the selected entry.
    fn enter_selected(&mut self) {
        self.status_bar.clear_status_message();

        // In picker mode, check if we should accept the selection instead of navigating
        if self.picker_config.is_picker() {
            if let Some(pane) = self.focused_pane() {
                if let Some(tab) = pane.active_tab() {
                    let selected = tab.selected_entries();
                    if !selected.is_empty() {
                        // If it's a directory in non-directory mode, navigate into it
                        if !self.picker_config.mode.is_directory_mode() {
                            let first = &selected[0];
                            if first.is_dir() {
                                // Fall through to normal enter behavior
                            } else {
                                // File selected - accept it
                                self.accept_picker_selection();
                                return;
                            }
                        }
                    }
                }
            }
        }

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
        self.status_bar.clear_status_message();
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
        self.status_bar.clear_status_message();
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
        self.status_bar.clear_status_message();
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
        self.status_bar.clear_status_message();
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

    // === File Operations ===

    /// Copy selected files to clipboard.
    fn copy_selected(&mut self) {
        let paths = self.get_selected_paths();
        if !paths.is_empty() {
            let count = paths.len();

            // Update internal clipboard
            self.clipboard.copy(paths.clone());

            // Update X11 system clipboard so other apps can paste
            if let Err(e) = self.x11_clipboard.set_files(&paths, false) {
                tracing::warn!("Failed to set X11 clipboard: {}", e);
            }

            let msg = if count == 1 { "1 item copied".to_string() } else { format!("{} items copied", count) };
            self.status_bar.set_status_message(msg);
            self.update_status_bar();
        }
    }

    /// Cut selected files to clipboard.
    fn cut_selected(&mut self) {
        let paths = self.get_selected_paths();
        if !paths.is_empty() {
            let count = paths.len();

            // Update internal clipboard
            self.clipboard.cut(paths.clone());

            // Update X11 system clipboard with cut flag so other apps know to move
            if let Err(e) = self.x11_clipboard.set_files(&paths, true) {
                tracing::warn!("Failed to set X11 clipboard: {}", e);
            }

            let msg = if count == 1 { "1 item cut".to_string() } else { format!("{} items cut", count) };
            self.status_bar.set_status_message(msg);
            self.update_status_bar();
        }
    }

    /// Paste files from clipboard to current directory.
    fn paste(&mut self) {
        let dest_dir = self.focused_pane()
            .and_then(|p| p.active_tab())
            .map(|t| t.current_path().clone());

        let dest_dir = match dest_dir {
            Some(d) => d,
            None => return,
        };

        if let Some((files, op)) = self.clipboard.take() {
            // Check for conflicts first
            let conflicts: Vec<PathBuf> = files.iter()
                .filter_map(|f| {
                    f.file_name().and_then(|name| {
                        let dest = dest_dir.join(name);
                        if dest.exists() { Some(f.clone()) } else { None }
                    })
                })
                .collect();

            if !conflicts.is_empty() {
                // Ring bell to alert user
                self.bell();

                // Show conflict dialog for the first conflict
                let first_conflict_name = conflicts[0]
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                self.conflict_dialog.show(&first_conflict_name);

                // Store pending paste state
                self.pending_paste = Some(PendingPaste {
                    files,
                    operation: op,
                    dest_dir,
                    conflicts,
                });
                return;
            }

            // No conflicts - proceed with paste
            let count = files.len();
            let sources = files.clone();
            let result = match op {
                ClipboardOperation::Copy => copy_files(&files, &dest_dir),
                ClipboardOperation::Cut => move_files(&files, &dest_dir),
            };

            // Show result in status bar and record for undo
            if result.success && !result.processed.is_empty() {
                let action = if op == ClipboardOperation::Copy { "copied" } else { "moved" };
                let msg = if count == 1 { format!("1 item {}", action) } else { format!("{} items {}", count, action) };
                self.status_bar.set_status_message(msg);

                // Record for undo
                let undo_op = match op {
                    ClipboardOperation::Copy => FileOperation::Copy {
                        sources,
                        destinations: result.processed.clone(),
                    },
                    ClipboardOperation::Cut => FileOperation::Move {
                        sources,
                        destinations: result.processed.clone(),
                    },
                };
                self.undo_stack.push(undo_op);
            } else if !result.success {
                let msg = format!("Operation failed: {}", result.error.as_deref().unwrap_or("unknown error"));
                self.status_bar.set_status_message(msg);
            }

            self.refresh();
        }
    }

    /// Move files to a directory (used by drag-and-drop).
    fn move_files_to_directory(&mut self, files: Vec<PathBuf>, dest_dir: PathBuf) {
        // Filter out files that are already in the destination directory (same-directory drop = cancel)
        let files: Vec<PathBuf> = files.into_iter()
            .filter(|f| f.parent() != Some(dest_dir.as_path()))
            .collect();

        // If no files need moving, silently cancel
        if files.is_empty() {
            return;
        }

        // Check for conflicts first
        let conflicts: Vec<PathBuf> = files.iter()
            .filter_map(|f| {
                f.file_name().and_then(|name| {
                    let dest = dest_dir.join(name);
                    if dest.exists() { Some(f.clone()) } else { None }
                })
            })
            .collect();

        if !conflicts.is_empty() {
            // Ring bell to alert user
            self.bell();

            // Show conflict dialog for the first conflict
            let first_conflict_name = conflicts[0]
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            self.conflict_dialog.show(&first_conflict_name);

            // Store pending paste state (reuse for drag-drop moves)
            self.pending_paste = Some(PendingPaste {
                files,
                operation: ClipboardOperation::Cut, // Move operation
                dest_dir,
                conflicts,
            });
            return;
        }

        // No conflicts - proceed with move
        let count = files.len();
        let sources = files.clone();
        let result = move_files(&files, &dest_dir);

        // Show result in status bar and record for undo
        if result.success && !result.processed.is_empty() {
            let msg = if count == 1 { "1 item moved".to_string() } else { format!("{} items moved", count) };
            self.status_bar.set_status_message(msg);

            // Record for undo
            let undo_op = FileOperation::Move {
                sources,
                destinations: result.processed.clone(),
            };
            self.undo_stack.push(undo_op);
        } else if !result.success {
            let msg = format!("Move failed: {}", result.error.as_deref().unwrap_or("unknown error"));
            self.status_bar.set_status_message(msg);
        }

        self.refresh();
    }

    /// Move selected files to trash.
    fn trash_selected(&mut self) {
        let paths = self.get_selected_paths();
        if paths.is_empty() {
            return;
        }

        let count = paths.len();
        let results = trash_files(&paths);
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failed_count = results.iter().filter(|r| r.is_err()).count();

        // Collect successful trash operations for undo
        let mut trashed_originals = Vec::new();
        let mut trash_names = Vec::new();
        for (i, result) in results.iter().enumerate() {
            if let Ok(trash_path) = result {
                trashed_originals.push(paths[i].clone());
                // Extract the trash entry name (filename in trash/files/)
                if let Some(name) = trash_path.file_name() {
                    trash_names.push(name.to_string_lossy().to_string());
                }
            }
        }

        if failed_count > 0 {
            let msg = format!("Moved {} to trash, {} failed", success_count, failed_count);
            self.status_bar.set_status_message(msg);
        } else {
            let msg = if count == 1 { "1 item moved to trash".to_string() } else { format!("{} items moved to trash", count) };
            self.status_bar.set_status_message(msg);
        }

        // Record for undo if any succeeded
        if !trashed_originals.is_empty() {
            self.undo_stack.push(FileOperation::Trash {
                originals: trashed_originals,
                trash_names,
            });
        }

        self.refresh();
    }

    /// Delete selected files permanently (shows confirmation dialog).
    fn delete_selected_permanently(&mut self) {
        let paths = self.get_selected_paths();
        if paths.is_empty() {
            return;
        }

        // Store paths and show confirmation dialog
        self.pending_delete_paths = paths.clone();
        self.confirm_dialog.show_delete_confirm(paths.len());
    }

    /// Handle confirmation dialog result.
    fn handle_dialog_result(&mut self, result: DialogResult) {
        match result {
            DialogResult::Confirmed => {
                // Perform the pending delete
                if !self.pending_delete_paths.is_empty() {
                    let paths = std::mem::take(&mut self.pending_delete_paths);
                    let count = paths.len();
                    let result = delete_files(&paths);

                    if result.success {
                        let msg = if count == 1 { "1 item deleted".to_string() } else { format!("{} items deleted", count) };
                        self.status_bar.set_status_message(msg);
                    } else {
                        let msg = format!("Delete failed: {}", result.error.as_deref().unwrap_or("unknown error"));
                        self.status_bar.set_status_message(msg);
                    }

                    self.refresh();
                }
            }
            DialogResult::Cancelled => {
                // Clear pending paths
                self.pending_delete_paths.clear();
            }
        }
    }

    /// Handle input dialog result.
    fn handle_input_result(&mut self, result: InputResult) {
        match result {
            InputResult::Submitted(value) => {
                // Currently only used for "Open With" custom application
                self.open_with_custom(&value);
            }
            InputResult::Cancelled => {
                self.pending_open_with_path = None;
            }
        }
    }

    /// Handle app picker result.
    fn handle_app_picker_result(&mut self, result: AppPickerResult) {
        match result {
            AppPickerResult::Selected(exec) => {
                // Open the pending file with the selected application
                self.open_with_custom(&exec);
            }
            AppPickerResult::Cancelled => {
                self.pending_open_with_path = None;
            }
        }
    }

    /// Handle conflict dialog result.
    fn handle_conflict_action(&mut self, action: ConflictAction) {
        let pending = match self.pending_paste.take() {
            Some(p) => p,
            None => return,
        };

        let apply_to_all = self.conflict_dialog.apply_to_all();

        match action {
            ConflictAction::Cancel => {
                // Cancel the entire operation
                self.status_bar.set_status_message("Paste cancelled");
            }
            ConflictAction::Skip => {
                // Skip conflicting files, paste the rest
                self.complete_paste_with_skip(pending, apply_to_all);
            }
            ConflictAction::Replace => {
                // Replace conflicting files
                self.complete_paste_with_replace(pending, apply_to_all);
            }
            ConflictAction::KeepBoth => {
                // Auto-rename and paste all
                self.complete_paste_with_rename(pending, apply_to_all);
            }
        }
    }

    /// Complete paste, skipping conflicts.
    fn complete_paste_with_skip(&mut self, pending: PendingPaste, _apply_to_all: bool) {
        let non_conflicting: Vec<_> = pending.files.iter()
            .filter(|f| !pending.conflicts.contains(f))
            .cloned()
            .collect();

        if non_conflicting.is_empty() {
            self.status_bar.set_status_message("All files skipped (conflicts)");
            return;
        }

        let result = match pending.operation {
            ClipboardOperation::Copy => copy_files(&non_conflicting, &pending.dest_dir),
            ClipboardOperation::Cut => move_files(&non_conflicting, &pending.dest_dir),
        };

        let skipped = pending.conflicts.len();
        if result.success {
            let action = if pending.operation == ClipboardOperation::Copy { "copied" } else { "moved" };
            self.status_bar.set_status_message(format!("{} {} (skipped {})", non_conflicting.len(), action, skipped));
        }
        self.refresh();
    }

    /// Complete paste, replacing conflicts.
    fn complete_paste_with_replace(&mut self, pending: PendingPaste, _apply_to_all: bool) {
        // Delete conflicting files first
        for conflict in &pending.conflicts {
            if let Some(name) = conflict.file_name() {
                let dest = pending.dest_dir.join(name);
                let _ = std::fs::remove_file(&dest).or_else(|_| std::fs::remove_dir_all(&dest));
            }
        }

        let result = match pending.operation {
            ClipboardOperation::Copy => copy_files(&pending.files, &pending.dest_dir),
            ClipboardOperation::Cut => move_files(&pending.files, &pending.dest_dir),
        };

        if result.success {
            let action = if pending.operation == ClipboardOperation::Copy { "copied" } else { "moved" };
            self.status_bar.set_status_message(format!("{} {} (replaced {})", pending.files.len(), action, pending.conflicts.len()));
        }
        self.refresh();
    }

    /// Complete paste, auto-renaming conflicts with inline rename prompt.
    fn complete_paste_with_rename(&mut self, mut pending: PendingPaste, apply_to_all: bool) {
        use garfield::core::make_unique_name;

        // First, paste all non-conflicting files
        let non_conflicting: Vec<_> = pending.files.iter()
            .filter(|f| !pending.conflicts.contains(f))
            .cloned()
            .collect();

        let mut sources = Vec::new();
        let mut destinations = Vec::new();

        for file in &non_conflicting {
            if let Some(name) = file.file_name() {
                let dest = pending.dest_dir.join(name);
                let result = match pending.operation {
                    ClipboardOperation::Copy => {
                        if file.is_dir() {
                            garfield::core::copy_path(file, &pending.dest_dir)
                        } else {
                            std::fs::copy(file, &dest).map(|_| dest.clone())
                        }
                    }
                    ClipboardOperation::Cut => {
                        std::fs::rename(file, &dest).map(|_| dest.clone())
                    }
                };
                if let Ok(dest_path) = result {
                    sources.push(file.clone());
                    destinations.push(dest_path);
                }
            }
        }

        // Now handle the first conflict - paste with suggested name and start rename
        if let Some(conflict_file) = pending.conflicts.first().cloned() {
            if let Some(name) = conflict_file.file_name() {
                let name_str = name.to_string_lossy();
                let unique_name = make_unique_name(&pending.dest_dir, &name_str);
                let final_dest = pending.dest_dir.join(&unique_name);

                // Perform the copy/move with the unique name
                let result = match pending.operation {
                    ClipboardOperation::Copy => {
                        // Copy directly to the unique destination path
                        garfield::core::copy_to_path(&conflict_file, &final_dest)
                    }
                    ClipboardOperation::Cut => {
                        std::fs::rename(&conflict_file, &final_dest).map(|_| final_dest.clone())
                    }
                };

                if let Ok(dest_path) = result {
                    sources.push(conflict_file.clone());
                    destinations.push(dest_path.clone());

                    // Record partial undo
                    if !destinations.is_empty() {
                        let undo_op = match pending.operation {
                            ClipboardOperation::Copy => FileOperation::Copy {
                                sources: sources.clone(),
                                destinations: destinations.clone(),
                            },
                            ClipboardOperation::Cut => FileOperation::Move {
                                sources: sources.clone(),
                                destinations: destinations.clone(),
                            },
                        };
                        self.undo_stack.push(undo_op);
                    }

                    // Refresh to show the new file
                    self.refresh();

                    // Select the newly pasted file and start rename
                    let found = if let Some(pane) = self.focused_pane_mut() {
                        if let Some(tab) = pane.active_tab_mut() {
                            // Find and select the file by name
                            if tab.select_by_name(&unique_name) {
                                // Start rename with the suggested name pre-populated
                                tab.start_rename_with_text(&unique_name);
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !found {
                        self.status_bar.set_status_message(format!("Pasted as '{}' - press F2 to rename", unique_name));
                    }

                    // Store remaining conflicts if not apply_to_all
                    if !apply_to_all && pending.conflicts.len() > 1 {
                        pending.conflicts.remove(0);
                        pending.files.retain(|f| pending.conflicts.contains(f));
                        self.pending_paste = Some(pending);
                        self.status_bar.set_status_message("Rename file, then Ctrl+V to continue");
                    } else if apply_to_all && pending.conflicts.len() > 1 {
                        // Auto-rename remaining conflicts silently
                        for conflict_file in pending.conflicts.iter().skip(1) {
                            if let Some(name) = conflict_file.file_name() {
                                let unique = make_unique_name(&pending.dest_dir, &name.to_string_lossy());
                                let dest = pending.dest_dir.join(&unique);
                                let _ = match pending.operation {
                                    ClipboardOperation::Copy => {
                                        if conflict_file.is_dir() {
                                            garfield::core::copy_path(conflict_file, &pending.dest_dir)
                                        } else {
                                            std::fs::copy(conflict_file, &dest).map(|_| dest)
                                        }
                                    }
                                    ClipboardOperation::Cut => {
                                        std::fs::rename(conflict_file, &dest).map(|_| dest)
                                    }
                                };
                            }
                        }
                        self.refresh();
                    }
                    return;
                }
            }
        }

        // No conflicts or all handled
        if !destinations.is_empty() {
            let undo_op = match pending.operation {
                ClipboardOperation::Copy => FileOperation::Copy { sources, destinations },
                ClipboardOperation::Cut => FileOperation::Move { sources, destinations },
            };
            self.undo_stack.push(undo_op);
        }

        self.refresh();
    }

    /// Ring the terminal bell.
    fn bell(&self) {
        // X11 bell
        let _ = self.window.connection().inner().bell(0);
        let _ = self.window.connection().flush();
    }

    /// Handle auto-enter when file drag flash completes.
    /// Enters the directory or switches to the tab, then continues dragging.
    fn handle_file_drag_auto_enter(&mut self, target: DragTarget) {
        match target {
            DragTarget::Directory { path, .. } => {
                // Navigate into the directory
                self.navigate_to(path);
                // Continue dragging in the new directory
                self.file_drag.continue_after_enter();
            }
            DragTarget::Tab { index, .. } => {
                // Switch to the target tab
                if let Some(pane) = self.focused_pane_mut() {
                    pane.set_active_tab(index);
                }
                self.sync_tab_bar();
                self.sync_breadcrumb();
                self.update_status_bar();
                // Continue dragging in the new tab
                self.file_drag.continue_after_enter();
            }
            DragTarget::Breadcrumb { path, .. } => {
                // Navigate to the breadcrumb segment directory
                self.navigate_to(path);
                // Continue dragging in the new directory
                self.file_drag.continue_after_enter();
            }
        }
    }

    /// Detect a valid drag target at the given position.
    /// Checks tabs first (for switching), then directories in the view.
    fn detect_file_drag_target(&self, pos: Point) -> Option<DragTarget> {
        // Get the paths being dragged to exclude them as targets
        let dragged_paths = self.file_drag.dragged_paths()
            .map(|paths| paths.clone())
            .unwrap_or_default();

        // Check tab bar first - can drop on any tab except if it contains the dragged item
        if let Some(tab_index) = self.tab_bar.tab_at_point(pos) {
            // Get the target path for this tab
            if let Some(pane) = self.focused_pane() {
                let tabs = pane.tabs();
                if let Some(tab) = tabs.get(tab_index) {
                    let target_path = tab.current_path().clone();
                    // Don't allow dropping into the same directory where items came from
                    if let Some(current_tab) = pane.active_tab() {
                        if current_tab.current_path() != &target_path {
                            return Some(DragTarget::Tab {
                                index: tab_index,
                                target_path,
                            });
                        }
                    }
                }
            }
        }

        // Check breadcrumb segments - can navigate to any parent directory
        if let Some((path, bounds)) = self.breadcrumb.segment_at_point(pos) {
            // Don't target current directory
            if let Some(pane) = self.focused_pane() {
                if let Some(tab) = pane.active_tab() {
                    if tab.current_path() != &path {
                        return Some(DragTarget::Breadcrumb { path, bounds });
                    }
                }
            }
        }

        // Check directory entries in the current view
        if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                if let Some((entry, bounds)) = tab.entry_bounds_at_point(pos) {
                    // Only directories are valid drop targets
                    if entry.is_dir() {
                        // Don't allow dropping on itself or a dragged item
                        if !dragged_paths.iter().any(|p| p == &entry.path) {
                            return Some(DragTarget::Directory {
                                path: entry.path.clone(),
                                bounds,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    /// Show the context menu at the given position.
    fn show_context_menu(&mut self, pos: Point) {
        // Check if we're in the Trash folder
        let in_trash = self.focused_pane()
            .and_then(|p| p.active_tab())
            .map(|t| {
                if let Some(trash_dir) = garfield::core::trash_dir() {
                    t.current_path().starts_with(&trash_dir)
                } else {
                    false
                }
            })
            .unwrap_or(false);

        // First, check what's under the cursor and potentially select it
        let (context_type, selected_count) = if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                if let Some(entry) = tab.entry_at_point(pos).cloned() {
                    // Right-clicked on an item
                    let selected = tab.selected_paths();

                    if selected.len() > 1 && selected.contains(&entry.path) {
                        // Item is part of multi-selection, keep it
                        (ContextType::MultiSelection, selected.len())
                    } else {
                        // Select this item (replaces current selection)
                        tab.select_by_name(&entry.name);

                        if entry.is_dir() {
                            (ContextType::Folder, 1)
                        } else {
                            (ContextType::File, 1)
                        }
                    }
                } else {
                    // Clicked on empty space - clear selection
                    tab.clear_selection();
                    (ContextType::EmptySpace, 0)
                }
            } else {
                (ContextType::EmptySpace, 0)
            }
        } else {
            (ContextType::EmptySpace, 0)
        };

        let has_clipboard = self.clipboard.has_files();
        self.context_menu.show(pos, context_type, selected_count, has_clipboard, in_trash);
    }

    /// Handle a context menu action.
    fn handle_context_menu_action(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::Open => self.enter_selected(),
            ContextMenuAction::OpenWith(app) => self.open_with(&app),
            ContextMenuAction::OpenInNewTab => self.open_in_new_tab(),
            ContextMenuAction::Copy | ContextMenuAction::CopyAll => self.copy_selected(),
            ContextMenuAction::Cut | ContextMenuAction::CutAll => self.cut_selected(),
            ContextMenuAction::Duplicate => self.duplicate_selected(),
            ContextMenuAction::Rename => self.start_rename(),
            ContextMenuAction::Trash | ContextMenuAction::TrashAll => self.trash_selected(),
            ContextMenuAction::Delete | ContextMenuAction::DeleteAll => self.delete_selected_permanently(),
            ContextMenuAction::Properties => self.show_properties(),
            ContextMenuAction::NewFile => self.create_new_file(),
            ContextMenuAction::NewFolder => self.create_new_folder(),
            ContextMenuAction::Paste => self.paste(),
            ContextMenuAction::Refresh => self.refresh(),
            ContextMenuAction::ViewList => self.set_view_mode(ViewMode::List),
            ContextMenuAction::ViewGrid => self.set_view_mode(ViewMode::Grid),
            ContextMenuAction::ViewColumns => self.set_view_mode(ViewMode::Columns),
            ContextMenuAction::SortByName => self.set_sort_order(garfield::core::SortOrder::Name),
            ContextMenuAction::SortBySize => self.set_sort_order(garfield::core::SortOrder::Size),
            ContextMenuAction::SortByDate => self.set_sort_order(garfield::core::SortOrder::Modified),
            ContextMenuAction::SortByType => self.set_sort_order(garfield::core::SortOrder::Type),
        }
    }

    /// Open selected item with a specific application.
    fn open_with(&mut self, app: &str) {
        let paths = self.get_selected_paths();
        let Some(path) = paths.first().cloned() else {
            return;
        };

        // Handle $CUSTOM - show app picker
        if app == "$CUSTOM" {
            self.pending_open_with_path = Some(path);
            self.app_picker.show();
            return;
        }

        // Resolve special application identifiers
        let resolved_app = match app {
            "$EDITOR" => self.resolve_text_editor(),
            other => Some(other.to_string()),
        };

        let Some(app_cmd) = resolved_app else {
            self.status_bar.set_status_message("No suitable application found");
            return;
        };

        match std::process::Command::new(&app_cmd).arg(&path).spawn() {
            Ok(_) => self.status_bar.set_status_message(format!("Opened with {}", app_cmd)),
            Err(e) => self.status_bar.set_status_message(format!("Failed to open with {}: {}", app_cmd, e)),
        }
    }

    /// Open file with custom application (from input dialog).
    fn open_with_custom(&mut self, app_name: &str) {
        let Some(path) = self.pending_open_with_path.take() else {
            return;
        };

        if app_name.is_empty() {
            self.status_bar.set_status_message("No application specified");
            return;
        }

        match std::process::Command::new(app_name).arg(&path).spawn() {
            Ok(_) => self.status_bar.set_status_message(format!("Opened with {}", app_name)),
            Err(e) => self.status_bar.set_status_message(format!("Failed to open with {}: {}", app_name, e)),
        }
    }

    /// Resolve text editor from environment or common editors.
    fn resolve_text_editor(&self) -> Option<String> {
        // Check environment variables first
        if let Ok(editor) = std::env::var("VISUAL") {
            if !editor.is_empty() && self.command_exists(&editor) {
                return Some(editor);
            }
        }
        if let Ok(editor) = std::env::var("EDITOR") {
            if !editor.is_empty() && self.command_exists(&editor) {
                return Some(editor);
            }
        }

        // Try common GUI text editors
        let editors = ["gedit", "kate", "mousepad", "xed", "pluma", "leafpad", "featherpad", "geany", "xfce4-terminal"];
        for editor in editors {
            if self.command_exists(editor) {
                return Some(editor.to_string());
            }
        }

        // Fallback to xdg-open
        Some("xdg-open".to_string())
    }

    /// Check if a command exists in PATH.
    fn command_exists(&self, cmd: &str) -> bool {
        // Extract just the command name (in case it's a full path or has args)
        let cmd_name = cmd.split_whitespace().next().unwrap_or(cmd);
        std::process::Command::new("which")
            .arg(cmd_name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Open folder in new tab.
    fn open_in_new_tab(&mut self) {
        let entry_path = self.focused_pane()
            .and_then(|p| p.active_tab())
            .and_then(|t| t.selected_entry())
            .filter(|e| e.is_dir())
            .map(|e| e.path.clone());

        if let Some(path) = entry_path {
            if let Some(pane) = self.focused_pane_mut() {
                pane.add_tab(path);
            }
            self.sync_tab_bar();
            self.sync_breadcrumb();
            self.update_status_bar();
        }
    }

    /// Show properties dialog (placeholder).
    fn show_properties(&mut self) {
        self.status_bar.set_status_message("Properties dialog not implemented");
    }

    /// Set sort order from context menu.
    fn set_sort_order(&mut self, order: garfield::core::SortOrder) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                let current_dir = tab.sort_direction();
                // Toggle direction if same order
                let new_dir = if tab.sort_order() == order {
                    match current_dir {
                        garfield::core::SortDirection::Ascending => garfield::core::SortDirection::Descending,
                        garfield::core::SortDirection::Descending => garfield::core::SortDirection::Ascending,
                    }
                } else {
                    garfield::core::SortDirection::Ascending
                };
                tab.set_sort(order, new_dir);
            }
        }
    }

    /// Create a new folder in the current directory.
    fn create_new_folder(&mut self) {
        let current_dir = self.focused_pane()
            .and_then(|p| p.active_tab())
            .map(|t| t.current_path().clone());

        let current_dir = match current_dir {
            Some(d) => d,
            None => return,
        };

        // Generate unique name
        let base_name = "New Folder";
        let mut name = base_name.to_string();
        let mut counter = 1;
        while current_dir.join(&name).exists() {
            name = format!("{} ({})", base_name, counter);
            counter += 1;
        }

        match create_directory(&current_dir, &name) {
            Ok(path) => {
                self.status_bar.set_status_message(format!("Created '{}'", name));
                self.undo_stack.push(FileOperation::CreateDir { path });
                self.refresh();
                // TODO: Start rename on the new folder
            }
            Err(e) => {
                self.status_bar.set_status_message(format!("Failed to create folder: {}", e));
            }
        }
    }

    /// Create a new empty file in the current directory.
    fn create_new_file(&mut self) {
        use garfield::core::make_unique_name;

        let current_dir = self.focused_pane()
            .and_then(|p| p.active_tab())
            .map(|t| t.current_path().clone());

        let current_dir = match current_dir {
            Some(d) => d,
            None => return,
        };

        // Generate unique name
        let unique_name = make_unique_name(&current_dir, "New File");
        let path = current_dir.join(&unique_name);

        match std::fs::File::create(&path) {
            Ok(_) => {
                self.status_bar.set_status_message(format!("Created '{}'", unique_name));
                self.refresh();

                // Select the new file and start rename
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        if tab.select_by_name(&unique_name) {
                            tab.start_rename();
                        }
                    }
                }
            }
            Err(e) => {
                self.status_bar.set_status_message(format!("Failed to create file: {}", e));
            }
        }
    }

    /// Duplicate the selected files/folders in the current directory.
    fn duplicate_selected(&mut self) {
        use garfield::core::make_unique_name;

        let selected = self.get_selected_paths();
        if selected.is_empty() {
            self.status_bar.set_status_message("No items selected");
            return;
        }

        let dest_dir = self.focused_pane()
            .and_then(|p| p.active_tab())
            .map(|t| t.current_path().clone());

        let dest_dir = match dest_dir {
            Some(d) => d,
            None => return,
        };

        let mut success_count = 0;
        let mut last_created_name = String::new();

        for path in &selected {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                let unique_name = make_unique_name(&dest_dir, &name_str);
                let dest = dest_dir.join(&unique_name);

                let result = if path.is_dir() {
                    garfield::core::copy_to_path(path, &dest)
                } else {
                    std::fs::copy(path, &dest).map(|_| dest.clone())
                };

                if result.is_ok() {
                    success_count += 1;
                    last_created_name = unique_name;
                }
            }
        }

        if success_count > 0 {
            let msg = if success_count == 1 {
                format!("Duplicated as '{}'", last_created_name)
            } else {
                format!("Duplicated {} items", success_count)
            };
            self.status_bar.set_status_message(msg);
            self.refresh();

            // Select the last duplicated item
            if !last_created_name.is_empty() {
                if let Some(pane) = self.focused_pane_mut() {
                    if let Some(tab) = pane.active_tab_mut() {
                        tab.select_by_name(&last_created_name);
                    }
                }
            }
        } else {
            self.status_bar.set_status_message("Failed to duplicate items");
        }
    }

    /// Undo the last file operation.
    fn undo(&mut self) {
        let op = match self.undo_stack.pop_undo() {
            Some(op) => op,
            None => {
                self.status_bar.set_status_message("Nothing to undo");
                return;
            }
        };

        let result = self.perform_undo(&op);
        match result {
            Ok(msg) => {
                self.status_bar.set_status_message(format!("Undo: {}", msg));
                self.undo_stack.push_redo(op);
            }
            Err(msg) => {
                self.status_bar.set_status_message(format!("Undo failed: {}", msg));
            }
        }
        self.refresh();
    }

    /// Perform the undo operation.
    fn perform_undo(&mut self, op: &FileOperation) -> Result<String, String> {
        use garfield::core::{delete_path, move_path, rename_path};

        match op {
            FileOperation::Copy { destinations, .. } => {
                // Undo copy: delete the copied files
                for dest in destinations {
                    if dest.exists() {
                        delete_path(dest).map_err(|e| e.to_string())?;
                    }
                }
                Ok(format!("Deleted {} copied item(s)", destinations.len()))
            }
            FileOperation::Move { sources, destinations } => {
                // Undo move: move files back to original locations
                for (src, dest) in sources.iter().zip(destinations.iter()) {
                    if dest.exists() {
                        if let Some(parent) = src.parent() {
                            move_path(dest, parent).map_err(|e| e.to_string())?;
                        }
                    }
                }
                Ok(format!("Moved {} item(s) back", sources.len()))
            }
            FileOperation::Trash { trash_names, .. } => {
                // Undo trash: restore from trash
                for name in trash_names {
                    restore_from_trash(name).map_err(|e| e.to_string())?;
                }
                Ok(format!("Restored {} item(s) from trash", trash_names.len()))
            }
            FileOperation::Rename { original, renamed } => {
                // Undo rename: rename back to original
                if renamed.exists() {
                    if let Some(orig_name) = original.file_name() {
                        rename_path(renamed, orig_name.to_string_lossy().as_ref())
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok("Renamed back".to_string())
            }
            FileOperation::CreateDir { path } => {
                // Undo create dir: delete the directory (only if empty)
                if path.exists() && path.is_dir() {
                    std::fs::remove_dir(path).map_err(|e| e.to_string())?;
                }
                Ok("Deleted folder".to_string())
            }
        }
    }

    /// Redo the last undone operation.
    fn redo(&mut self) {
        let op = match self.undo_stack.pop_redo() {
            Some(op) => op,
            None => {
                self.status_bar.set_status_message("Nothing to redo");
                return;
            }
        };

        let result = self.perform_redo(&op);
        match result {
            Ok(msg) => {
                self.status_bar.set_status_message(format!("Redo: {}", msg));
                self.undo_stack.push(op);
            }
            Err(msg) => {
                self.status_bar.set_status_message(format!("Redo failed: {}", msg));
            }
        }
        self.refresh();
    }

    /// Perform the redo operation.
    fn perform_redo(&mut self, op: &FileOperation) -> Result<String, String> {
        use garfield::core::{copy_path, move_path, rename_path};

        match op {
            FileOperation::Copy { sources, destinations } => {
                // Redo copy: copy files again
                for (src, dest) in sources.iter().zip(destinations.iter()) {
                    if src.exists() {
                        if let Some(parent) = dest.parent() {
                            copy_path(src, parent).map_err(|e| e.to_string())?;
                        }
                    }
                }
                Ok(format!("Copied {} item(s)", sources.len()))
            }
            FileOperation::Move { sources, destinations } => {
                // Redo move: move files again
                for (src, dest) in sources.iter().zip(destinations.iter()) {
                    if src.exists() {
                        if let Some(parent) = dest.parent() {
                            move_path(src, parent).map_err(|e| e.to_string())?;
                        }
                    }
                }
                Ok(format!("Moved {} item(s)", sources.len()))
            }
            FileOperation::Trash { originals, .. } => {
                // Redo trash: trash the files again
                let results = trash_files(originals);
                let success = results.iter().filter(|r| r.is_ok()).count();
                Ok(format!("Trashed {} item(s)", success))
            }
            FileOperation::Rename { original, renamed } => {
                // Redo rename: rename again
                if original.exists() {
                    if let Some(new_name) = renamed.file_name() {
                        rename_path(original, new_name.to_string_lossy().as_ref())
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok("Renamed".to_string())
            }
            FileOperation::CreateDir { path } => {
                // Redo create dir: create the directory again
                std::fs::create_dir(path).map_err(|e| e.to_string())?;
                Ok("Created folder".to_string())
            }
        }
    }

    /// Start inline rename for the selected file.
    fn start_rename(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.start_rename();
            }
        }
    }

    /// Check if rename is in progress.
    fn is_renaming(&self) -> bool {
        self.focused_pane()
            .and_then(|p| p.active_tab())
            .map_or(false, |t| t.is_renaming())
    }

    /// Cancel rename operation.
    fn cancel_rename(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.cancel_rename();
            }
        }
    }

    /// Confirm rename operation.
    fn confirm_rename(&mut self) {
        let result = self.focused_pane_mut()
            .and_then(|p| p.active_tab_mut())
            .map(|t| t.confirm_rename());

        match result {
            Some(Ok((original, renamed, new_name))) => {
                // Only record undo if the name actually changed
                if original != renamed {
                    self.undo_stack.push(FileOperation::Rename {
                        original,
                        renamed,
                    });
                }
                self.status_bar.set_status_message(format!("Renamed to '{}'", new_name));
                // Update status bar with new entry count
                self.update_status_bar();
            }
            Some(Err(msg)) => {
                self.status_bar.set_status_message(format!("Rename failed: {}", msg));
            }
            None => {}
        }
    }

    /// Get paths of all selected files.
    fn get_selected_paths(&self) -> Vec<PathBuf> {
        self.focused_pane()
            .and_then(|p| p.active_tab())
            .map(|t| t.selected_paths())
            .unwrap_or_default()
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
        self.sync_toolbar_icon_size();
        self.sync_breadcrumb();
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
            ToolbarAction::Copy => self.copy_selected(),
            ToolbarAction::Cut => self.cut_selected(),
            ToolbarAction::Paste => self.paste(),
            ToolbarAction::Trash => self.trash_selected(),
            ToolbarAction::NewFolder => self.create_new_folder(),
            ToolbarAction::IconSizeSmall => self.set_icon_size(IconSize::Small),
            ToolbarAction::IconSizeMedium => self.set_icon_size(IconSize::Medium),
            ToolbarAction::IconSizeLarge => self.set_icon_size(IconSize::Large),
        }
    }

    /// Set icon size for the active tab's grid view.
    fn set_icon_size(&mut self, size: IconSize) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                tab.set_icon_size(size);
            }
        }
        self.sync_toolbar_icon_size();
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

    /// Sync toolbar icon size with current tab's grid view icon size.
    fn sync_toolbar_icon_size(&mut self) {
        if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                let action = match tab.icon_size() {
                    IconSize::Small => ToolbarAction::IconSizeSmall,
                    IconSize::Medium => ToolbarAction::IconSizeMedium,
                    IconSize::Large => ToolbarAction::IconSizeLarge,
                };
                self.toolbar.set_active_icon_size(action);
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

    /// Process pending preview requests from column views.
    fn process_pending_previews(&mut self) {
        // Check focused pane's active tab for pending preview
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                if let Some((path, sort_order, sort_direction)) = tab.take_pending_preview() {
                    self.preview_loader.load(path, sort_order, sort_direction);
                }
            }
        }
    }

    /// Process pending image preview requests.
    fn process_pending_image_previews(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                if let Some((path, max_width, max_height)) = tab.take_pending_image_preview() {
                    self.image_preview_loader.load(path, max_width, max_height);
                }
            }
        }
    }

    /// Process pending PDF preview requests.
    fn process_pending_pdf_previews(&mut self) {
        if let Some(pane) = self.focused_pane_mut() {
            if let Some(tab) = pane.active_tab_mut() {
                if let Some((path, max_width, max_height)) = tab.take_pending_pdf_preview() {
                    self.pdf_preview_loader.load(path, max_width, max_height);
                }
            }
        }
    }

    /// Update status bar.
    fn update_status_bar(&mut self) {
        let stats = self.focused_pane()
            .and_then(|pane| pane.active_tab())
            .map(|tab| (tab.visible_count(), tab.selection_count(), tab.selected_size(), tab.view_mode().name(), tab.current_path().to_path_buf()));

        if let Some((visible_count, selected_count, selected_size, view_mode, path)) = stats {
            self.status_bar.update(visible_count, selected_count, selected_size);
            self.status_bar.set_view_mode(view_mode);
            self.status_bar.update_free_space(&path);

            // Update toolbar file ops state
            let has_selection = selected_count > 0;
            let has_clipboard = self.clipboard.has_files();
            self.toolbar.set_file_ops_state(has_selection, has_clipboard);
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

        let toolbar_bounds = Rect::new(
            sidebar_w as i32,
            TAB_BAR_HEIGHT as i32,
            width - sidebar_w,
            TOOLBAR_HEIGHT,
        );
        self.toolbar.set_bounds(toolbar_bounds);

        let breadcrumb_bounds = Rect::new(
            sidebar_w as i32,
            (TAB_BAR_HEIGHT + TOOLBAR_HEIGHT) as i32,
            width - sidebar_w,
            BREADCRUMB_HEIGHT,
        );
        self.breadcrumb.set_bounds(breadcrumb_bounds);
        self.address_bar.set_bounds(breadcrumb_bounds);

        // Calculate footer height (status bar + picker toolbar if present)
        let footer_height = if self.picker_toolbar.is_some() {
            PICKER_TOOLBAR_HEIGHT  // Picker toolbar replaces status bar
        } else {
            STATUS_BAR_HEIGHT
        };

        let content_bounds = Rect::new(
            sidebar_w as i32,
            header_height as i32,
            width - sidebar_w,
            height - header_height - footer_height,
        );
        self.root_pane.set_bounds(content_bounds);

        // Update picker toolbar bounds at bottom (if in picker mode)
        if let Some(ref mut picker_toolbar) = self.picker_toolbar {
            picker_toolbar.set_bounds(Rect::new(
                sidebar_w as i32,
                (height - PICKER_TOOLBAR_HEIGHT) as i32,
                width - sidebar_w,
                PICKER_TOOLBAR_HEIGHT,
            ));
        } else {
            // Only show status bar when not in picker mode
            self.status_bar.set_bounds(Rect::new(
                sidebar_w as i32,
                (height - STATUS_BAR_HEIGHT) as i32,
                width - sidebar_w,
                STATUS_BAR_HEIGHT,
            ));
        }

        self.help_modal.set_bounds(Rect::new(0, 0, width, height));
        self.confirm_dialog.set_bounds(Rect::new(0, 0, width, height));
        self.conflict_dialog.set_bounds(Rect::new(0, 0, width, height));
        self.progress_dialog.set_bounds(Rect::new(0, 0, width, height));
        self.context_menu.set_bounds(Rect::new(0, 0, width, height));
        self.input_dialog.set_bounds(Rect::new(0, 0, width, height));
        self.app_picker.set_bounds(Rect::new(0, 0, width, height));
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

        // Update and draw toolbar (or picker toolbar)
        let (can_back, can_forward) = if let Some(pane) = self.focused_pane() {
            if let Some(tab) = pane.active_tab() {
                (tab.can_go_back(), tab.can_go_forward())
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };

        // Always render the regular toolbar
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

        // Draw status bar or picker toolbar at bottom
        if self.picker_toolbar.is_some() {
            // Picker mode: draw picker toolbar instead of status bar
            let has_valid_selection = self.has_valid_picker_selection();
            let picker_toolbar = self.picker_toolbar.as_mut().unwrap();
            picker_toolbar.set_accept_enabled(has_valid_selection);
            picker_toolbar.render(&self.renderer)?;
        } else {
            // Normal mode: draw status bar
            self.status_bar.render(&self.renderer)?;
        }

        // Draw toolbar tooltip overlay (on top of other UI)
        self.toolbar.render_tooltip_overlay(&self.renderer)?;

        // Draw drag label overlay (on top of other UI)
        self.render_drag_label()?;

        // Draw file drag overlay (drag label and target highlight)
        self.render_file_drag()?;

        // Draw help modal overlay (on top of everything)
        self.help_modal.render(&self.renderer)?;

        // Draw confirm dialog overlay (on top of everything)
        self.confirm_dialog.render(&self.renderer)?;

        // Draw conflict dialog overlay (on top of everything)
        self.conflict_dialog.render(&self.renderer)?;

        // Draw input dialog overlay (on top of everything)
        self.input_dialog.render(&self.renderer)?;

        // Draw app picker overlay (on top of everything)
        self.app_picker.render(&self.renderer)?;

        // Draw context menu overlay
        self.context_menu.render(&self.renderer)?;

        // Draw progress dialog overlay (on top of everything)
        self.progress_dialog.render(&self.renderer)?;

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

    /// Render file drag overlay (drag label and target highlight).
    fn render_file_drag(&self) -> Result<()> {
        // Only render if file drag is actively dragging
        if !self.file_drag.is_dragging() {
            return Ok(());
        }

        let theme = self.renderer.theme();

        // Render target highlight if hovering and highlight should be shown
        if self.file_drag.should_show_highlight() {
            if let Some(target) = self.file_drag.current_target() {
                match target {
                    DragTarget::Directory { bounds, .. } => {
                        // Draw highlight rectangle around the directory
                        let highlight_color = theme.selection_background.with_alpha(0.3);
                        self.renderer.fill_rect(*bounds, highlight_color)?;
                        self.renderer.stroke_rect(*bounds, theme.selection_background, 2.0)?;
                    }
                    DragTarget::Tab { index, .. } => {
                        // Draw highlight under the tab
                        if let Some(tab_bounds) = self.tab_bar.tab_bounds_at(*index) {
                            let highlight_color = theme.selection_background.with_alpha(0.3);
                            self.renderer.fill_rect(tab_bounds, highlight_color)?;
                            self.renderer.stroke_rect(tab_bounds, theme.selection_background, 2.0)?;
                        }
                    }
                    DragTarget::Breadcrumb { bounds, .. } => {
                        // Draw highlight around the breadcrumb segment
                        let highlight_color = theme.selection_background.with_alpha(0.3);
                        self.renderer.fill_rect(*bounds, highlight_color)?;
                        self.renderer.stroke_rect(*bounds, theme.selection_background, 2.0)?;
                    }
                }
            }
        }

        // Render drag label at cursor
        if let (Some(paths), Some(pos)) = (self.file_drag.dragged_paths(), self.file_drag.current_pos()) {
            let count = paths.len();
            let label = if count == 1 {
                paths.first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "1 item".to_string())
            } else {
                format!("{} items", count)
            };

            // Create text style for the drag label
            let text_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size)
                .color(theme.item_foreground);

            // Measure the text to size the background
            let text_size = self.renderer.measure_text(&label, &text_style)?;

            // Position the label slightly offset from the cursor
            let label_x = pos.x + 16;
            let label_y = pos.y + 8;
            let padding = 8;

            // Draw background
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

            // File icon prefix
            let icon_style = text_style.clone().color(theme.selection_background);
            self.renderer.text("≡", label_x as f64, label_y as f64, &icon_style)?;

            // Draw the text
            self.renderer.text(&label, (label_x + 14) as f64, label_y as f64, &text_style)?;
        }

        Ok(())
    }

    /// Blit the rendered surface to the window.
    fn blit_surface(&mut self) -> Result<()> {
        let size = self.renderer.size();
        let window_id = self.window.id();
        let depth = self.window.depth();
        let gc = self.gc;
        let conn = self.window.connection().clone();

        // Access surface data directly without copying, blit to X11
        self.renderer.surface_mut().with_data(|data| {
            let _ = conn.inner().put_image(
                ImageFormat::Z_PIXMAP,
                window_id,
                gc,
                size.width as u16,
                size.height as u16,
                0,
                0,
                0,
                depth,
                data,
            );
        })?;

        self.window.connection().flush()?;

        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.window.connection().inner().free_gc(self.gc);
    }
}
