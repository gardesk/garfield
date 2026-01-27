//! Context menu component for right-click actions.

use anyhow::Result;
use gartk_core::{Key, Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::time::Instant;

/// Menu item height.
const ITEM_HEIGHT: u32 = 28;

/// Separator height.
const SEPARATOR_HEIGHT: u32 = 9;

/// Hover delay before opening submenu (milliseconds).
const SUBMENU_HOVER_DELAY_MS: u64 = 300;

/// Minimum menu width.
const MIN_MENU_WIDTH: u32 = 200;

/// Padding inside menu.
const MENU_PADDING: u32 = 4;

/// Context type determining which menu items to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextType {
    /// Right-click on a file.
    File,
    /// Right-click on a folder.
    Folder,
    /// Right-click on empty space in file view.
    EmptySpace,
    /// Multi-selection context.
    MultiSelection,
}

/// A context menu action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuAction {
    // File/Folder actions
    Open,
    OpenWith(String),
    OpenInNewTab,
    Copy,
    Cut,
    Duplicate,
    Rename,
    Trash,
    Delete,
    Properties,

    // Empty space actions
    NewFile,
    NewFolder,
    Paste,
    Refresh,

    // View submenu actions
    ViewList,
    ViewGrid,
    ViewColumns,

    // Sort submenu actions
    SortByName,
    SortBySize,
    SortByDate,
    SortByType,

    // Multi-selection actions (same as single but for clarity)
    CopyAll,
    CutAll,
    TrashAll,
    DeleteAll,
}

/// A single menu item.
#[derive(Debug, Clone)]
pub enum MenuItem {
    /// Regular action item.
    Action {
        label: String,
        action: ContextMenuAction,
        shortcut: Option<String>,
        enabled: bool,
    },
    /// Separator line.
    Separator,
    /// Submenu with nested items.
    Submenu {
        label: String,
        items: Vec<MenuItem>,
    },
}

impl MenuItem {
    /// Create a new action item.
    pub fn action(label: &str, action: ContextMenuAction) -> Self {
        MenuItem::Action {
            label: label.to_string(),
            action,
            shortcut: None,
            enabled: true,
        }
    }

    /// Add a keyboard shortcut hint.
    pub fn with_shortcut(self, shortcut: &str) -> Self {
        match self {
            MenuItem::Action { label, action, enabled, .. } => MenuItem::Action {
                label,
                action,
                shortcut: Some(shortcut.to_string()),
                enabled,
            },
            other => other,
        }
    }

    /// Set enabled state.
    pub fn with_enabled(self, enabled: bool) -> Self {
        match self {
            MenuItem::Action { label, action, shortcut, .. } => MenuItem::Action {
                label,
                action,
                shortcut,
                enabled,
            },
            other => other,
        }
    }

    /// Create a submenu.
    pub fn submenu(label: &str, items: Vec<MenuItem>) -> Self {
        MenuItem::Submenu {
            label: label.to_string(),
            items,
        }
    }

    /// Create a separator.
    pub fn separator() -> Self {
        MenuItem::Separator
    }

    /// Check if this is a separator.
    fn is_separator(&self) -> bool {
        matches!(self, MenuItem::Separator)
    }

    /// Get the height of this item.
    fn height(&self) -> u32 {
        match self {
            MenuItem::Separator => SEPARATOR_HEIGHT,
            _ => ITEM_HEIGHT,
        }
    }
}

/// Rendered menu item with computed bounds.
struct RenderedItem {
    item: MenuItem,
    bounds: Rect,
}

/// Context menu state.
pub struct ContextMenu {
    /// Window bounds (for clipping/positioning).
    window_bounds: Rect,
    /// Menu bounds (position and size).
    menu_bounds: Rect,
    /// Whether the menu is visible.
    visible: bool,
    /// Context type.
    context_type: ContextType,
    /// Menu items.
    items: Vec<MenuItem>,
    /// Rendered items with bounds.
    rendered_items: Vec<RenderedItem>,
    /// Currently focused index (for keyboard nav).
    focused_index: Option<usize>,
    /// Hovered item index.
    hovered_index: Option<usize>,
    /// Open submenu index (if any).
    open_submenu_index: Option<usize>,
    /// Submenu focused index.
    submenu_focused_index: Option<usize>,
    /// Submenu hovered index.
    submenu_hovered_index: Option<usize>,
    /// Submenu items bounds.
    submenu_bounds: Option<Rect>,
    /// Submenu rendered items.
    submenu_rendered_items: Vec<RenderedItem>,
    /// Time when hover started on a submenu item (for delay).
    submenu_hover_start: Option<Instant>,
    /// Number of selected items (for multi-select labels).
    selected_count: usize,
    /// Whether clipboard has content (for paste enable).
    has_clipboard: bool,
    /// Whether we're in the Trash folder.
    in_trash: bool,
}

impl ContextMenu {
    /// Create a new context menu.
    pub fn new(window_bounds: Rect) -> Self {
        Self {
            window_bounds,
            menu_bounds: Rect::new(0, 0, MIN_MENU_WIDTH, 0),
            visible: false,
            context_type: ContextType::EmptySpace,
            items: Vec::new(),
            rendered_items: Vec::new(),
            focused_index: None,
            hovered_index: None,
            open_submenu_index: None,
            submenu_focused_index: None,
            submenu_hovered_index: None,
            submenu_bounds: None,
            submenu_rendered_items: Vec::new(),
            submenu_hover_start: None,
            selected_count: 0,
            has_clipboard: false,
            in_trash: false,
        }
    }

    /// Set window bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.window_bounds = bounds;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Hide the menu.
    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.rendered_items.clear();
        self.focused_index = None;
        self.hovered_index = None;
        self.open_submenu_index = None;
        self.submenu_focused_index = None;
        self.submenu_hovered_index = None;
        self.submenu_bounds = None;
        self.submenu_rendered_items.clear();
        self.submenu_hover_start = None;
    }

    /// Show the context menu at position.
    pub fn show(
        &mut self,
        pos: Point,
        context_type: ContextType,
        selected_count: usize,
        has_clipboard: bool,
        in_trash: bool,
    ) {
        self.context_type = context_type;
        self.selected_count = selected_count;
        self.has_clipboard = has_clipboard;
        self.in_trash = in_trash;

        // Build menu items based on context
        self.items = self.build_menu_items();

        // Calculate menu size
        let (width, height) = self.calculate_menu_size(&self.items);

        // Position menu (flip if near edges)
        let (x, y) = self.calculate_position(pos, width, height);

        self.menu_bounds = Rect::new(x, y, width, height);
        self.layout_items();

        self.visible = true;
        self.focused_index = Some(0);
        self.hovered_index = None;
        self.open_submenu_index = None;
    }

    /// Build menu items based on context type.
    fn build_menu_items(&self) -> Vec<MenuItem> {
        match self.context_type {
            ContextType::File => self.build_file_menu(),
            ContextType::Folder => self.build_folder_menu(),
            ContextType::EmptySpace => self.build_empty_space_menu(),
            ContextType::MultiSelection => self.build_multi_selection_menu(),
        }
    }

    fn build_file_menu(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::action("Open", ContextMenuAction::Open),
            MenuItem::submenu("Open With", self.build_open_with_submenu()),
            MenuItem::separator(),
            MenuItem::action("Copy", ContextMenuAction::Copy).with_shortcut("Ctrl+C"),
            MenuItem::action("Cut", ContextMenuAction::Cut).with_shortcut("Ctrl+X"),
            MenuItem::action("Duplicate", ContextMenuAction::Duplicate).with_shortcut("Ctrl+Shift+D"),
            MenuItem::separator(),
            MenuItem::action("Rename", ContextMenuAction::Rename).with_shortcut("F2"),
        ];

        // Only show "Move to Trash" if not already in Trash
        if !self.in_trash {
            items.push(MenuItem::action("Move to Trash", ContextMenuAction::Trash).with_shortcut("Del"));
        }
        items.push(MenuItem::action("Delete Permanently", ContextMenuAction::Delete).with_shortcut("Shift+Del"));
        items.push(MenuItem::separator());
        items.push(MenuItem::action("Properties", ContextMenuAction::Properties));

        items
    }

    fn build_folder_menu(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::action("Open", ContextMenuAction::Open),
            MenuItem::action("Open in New Tab", ContextMenuAction::OpenInNewTab),
            MenuItem::submenu("Open With", self.build_open_with_submenu()),
            MenuItem::separator(),
            MenuItem::action("Copy", ContextMenuAction::Copy).with_shortcut("Ctrl+C"),
            MenuItem::action("Cut", ContextMenuAction::Cut).with_shortcut("Ctrl+X"),
            MenuItem::action("Duplicate", ContextMenuAction::Duplicate).with_shortcut("Ctrl+Shift+D"),
            MenuItem::separator(),
            MenuItem::action("Rename", ContextMenuAction::Rename).with_shortcut("F2"),
        ];

        // Only show "Move to Trash" if not already in Trash
        if !self.in_trash {
            items.push(MenuItem::action("Move to Trash", ContextMenuAction::Trash).with_shortcut("Del"));
        }
        items.push(MenuItem::action("Delete Permanently", ContextMenuAction::Delete).with_shortcut("Shift+Del"));
        items.push(MenuItem::separator());
        items.push(MenuItem::action("Properties", ContextMenuAction::Properties));

        items
    }

    fn build_empty_space_menu(&self) -> Vec<MenuItem> {
        vec![
            MenuItem::action("New File", ContextMenuAction::NewFile).with_shortcut("Ctrl+Shift+F"),
            MenuItem::action("New Folder", ContextMenuAction::NewFolder).with_shortcut("Ctrl+Shift+N"),
            MenuItem::separator(),
            MenuItem::action("Paste", ContextMenuAction::Paste)
                .with_shortcut("Ctrl+V")
                .with_enabled(self.has_clipboard),
            MenuItem::separator(),
            MenuItem::action("Refresh", ContextMenuAction::Refresh).with_shortcut("F5"),
            MenuItem::separator(),
            MenuItem::submenu("View", self.build_view_submenu()),
            MenuItem::submenu("Sort By", self.build_sort_submenu()),
        ]
    }

    fn build_multi_selection_menu(&self) -> Vec<MenuItem> {
        let count = self.selected_count;
        let mut items = vec![
            MenuItem::action(&format!("Copy {} items", count), ContextMenuAction::CopyAll)
                .with_shortcut("Ctrl+C"),
            MenuItem::action(&format!("Cut {} items", count), ContextMenuAction::CutAll)
                .with_shortcut("Ctrl+X"),
            MenuItem::separator(),
        ];

        // Only show "Move to Trash" if not already in Trash
        if !self.in_trash {
            items.push(MenuItem::action(&format!("Move {} items to Trash", count), ContextMenuAction::TrashAll)
                .with_shortcut("Del"));
        }
        items.push(MenuItem::action(&format!("Delete {} items", count), ContextMenuAction::DeleteAll)
            .with_shortcut("Shift+Del"));

        items
    }

    fn build_open_with_submenu(&self) -> Vec<MenuItem> {
        vec![
            MenuItem::action("Default Application", ContextMenuAction::OpenWith("xdg-open".to_string())),
            MenuItem::action("Text Editor", ContextMenuAction::OpenWith("$EDITOR".to_string())),
            MenuItem::separator(),
            MenuItem::action("Other Application...", ContextMenuAction::OpenWith("$CUSTOM".to_string())),
        ]
    }

    fn build_view_submenu(&self) -> Vec<MenuItem> {
        vec![
            MenuItem::action("List", ContextMenuAction::ViewList).with_shortcut("Ctrl+1"),
            MenuItem::action("Grid", ContextMenuAction::ViewGrid).with_shortcut("Ctrl+2"),
            MenuItem::action("Columns", ContextMenuAction::ViewColumns).with_shortcut("Ctrl+3"),
        ]
    }

    fn build_sort_submenu(&self) -> Vec<MenuItem> {
        vec![
            MenuItem::action("Name", ContextMenuAction::SortByName),
            MenuItem::action("Size", ContextMenuAction::SortBySize),
            MenuItem::action("Date Modified", ContextMenuAction::SortByDate),
            MenuItem::action("Type", ContextMenuAction::SortByType),
        ]
    }

    /// Calculate menu dimensions.
    fn calculate_menu_size(&self, items: &[MenuItem]) -> (u32, u32) {
        let height: u32 = items.iter().map(|i| i.height()).sum::<u32>() + (MENU_PADDING * 2);
        let width = MIN_MENU_WIDTH + 60; // Extra space for shortcuts
        (width, height)
    }

    /// Calculate menu position, flipping if near window edges.
    fn calculate_position(&self, pos: Point, width: u32, height: u32) -> (i32, i32) {
        let mut x = pos.x;
        let mut y = pos.y;

        // Flip horizontally if menu would extend past right edge
        if x + width as i32 > self.window_bounds.x + self.window_bounds.width as i32 {
            x = pos.x - width as i32;
        }

        // Flip vertically if menu would extend past bottom edge
        if y + height as i32 > self.window_bounds.y + self.window_bounds.height as i32 {
            y = pos.y - height as i32;
        }

        // Ensure menu stays within bounds
        x = x.max(self.window_bounds.x);
        y = y.max(self.window_bounds.y);

        (x, y)
    }

    /// Layout items and calculate their bounds.
    fn layout_items(&mut self) {
        self.rendered_items.clear();
        let mut y = self.menu_bounds.y + MENU_PADDING as i32;

        for item in &self.items {
            let height = item.height();

            let bounds = Rect::new(
                self.menu_bounds.x + MENU_PADDING as i32,
                y,
                self.menu_bounds.width - MENU_PADDING * 2,
                height,
            );

            self.rendered_items.push(RenderedItem {
                item: item.clone(),
                bounds,
            });

            y += height as i32;
        }
    }

    /// Layout submenu items.
    fn layout_submenu(&mut self, parent_bounds: &Rect, items: &[MenuItem]) {
        let (width, height) = self.calculate_menu_size(items);

        // Try to open to the right
        let mut x = parent_bounds.x + parent_bounds.width as i32 - 4;
        let y = parent_bounds.y;

        // If would overflow right edge, open to left
        if x + width as i32 > self.window_bounds.x + self.window_bounds.width as i32 {
            x = self.menu_bounds.x - width as i32 + 4;
        }

        self.submenu_bounds = Some(Rect::new(x, y, width, height));
        self.submenu_rendered_items.clear();

        let mut item_y = y + MENU_PADDING as i32;
        for item in items {
            let item_height = item.height();
            let bounds = Rect::new(
                x + MENU_PADDING as i32,
                item_y,
                width - MENU_PADDING * 2,
                item_height,
            );
            self.submenu_rendered_items.push(RenderedItem {
                item: item.clone(),
                bounds,
            });
            item_y += item_height as i32;
        }
    }

    /// Handle mouse move. Returns true if state changed and redraw is needed.
    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        if !self.visible {
            return false;
        }

        let old_hovered = self.hovered_index;
        let old_submenu_hovered = self.submenu_hovered_index;
        let old_open_submenu = self.open_submenu_index;

        // Check submenu first if open
        if let Some(ref submenu_bounds) = self.submenu_bounds {
            if submenu_bounds.contains_point(pos) {
                // Mouse is in submenu
                self.submenu_hovered_index = self.submenu_rendered_items
                    .iter()
                    .position(|r| r.bounds.contains_point(pos) && !r.item.is_separator());
                if let Some(idx) = self.submenu_hovered_index {
                    self.submenu_focused_index = Some(idx);
                }
                return self.submenu_hovered_index != old_submenu_hovered;
            }
        }

        // Check main menu
        self.submenu_hovered_index = None;

        for (i, rendered) in self.rendered_items.iter().enumerate() {
            if rendered.bounds.contains_point(pos) && !rendered.item.is_separator() {
                self.hovered_index = Some(i);
                self.focused_index = Some(i);

                // Handle submenu hover
                if matches!(&rendered.item, MenuItem::Submenu { .. }) {
                    if self.open_submenu_index != Some(i) {
                        if self.submenu_hover_start.is_none() {
                            self.submenu_hover_start = Some(Instant::now());
                        } else if let Some(start) = self.submenu_hover_start {
                            if start.elapsed().as_millis() > SUBMENU_HOVER_DELAY_MS as u128 {
                                self.open_submenu(i);
                            }
                        }
                    }
                } else {
                    // Not a submenu, close any open submenu
                    self.submenu_hover_start = None;
                    if self.open_submenu_index.is_some() {
                        self.close_submenu();
                    }
                }
                return self.hovered_index != old_hovered
                    || self.submenu_hovered_index != old_submenu_hovered
                    || self.open_submenu_index != old_open_submenu;
            }
        }

        // Mouse not over any item
        self.hovered_index = None;

        // Check if mouse left menu area entirely
        if !self.menu_bounds.contains_point(pos) {
            if let Some(ref submenu_bounds) = self.submenu_bounds {
                if !submenu_bounds.contains_point(pos) {
                    self.submenu_hover_start = None;
                }
            } else {
                self.submenu_hover_start = None;
            }
        }

        self.hovered_index != old_hovered
            || self.submenu_hovered_index != old_submenu_hovered
            || self.open_submenu_index != old_open_submenu
    }

    /// Open a submenu by index.
    fn open_submenu(&mut self, index: usize) {
        // Get the bounds and items from the rendered item
        let (bounds, items) = if let Some(rendered) = self.rendered_items.get(index) {
            if let MenuItem::Submenu { items, .. } = &rendered.item {
                (rendered.bounds, items.clone())
            } else {
                return;
            }
        } else {
            return;
        };

        self.layout_submenu(&bounds, &items);
        self.open_submenu_index = Some(index);
        self.submenu_focused_index = Some(0);
        self.submenu_hovered_index = None;
    }

    /// Close the open submenu.
    fn close_submenu(&mut self) {
        self.open_submenu_index = None;
        self.submenu_focused_index = None;
        self.submenu_hovered_index = None;
        self.submenu_bounds = None;
        self.submenu_rendered_items.clear();
    }

    /// Handle click. Returns action if item was clicked.
    pub fn on_click(&mut self, pos: Point) -> Option<ContextMenuAction> {
        if !self.visible {
            return None;
        }

        // Check submenu click first
        if let Some(ref submenu_bounds) = self.submenu_bounds {
            if submenu_bounds.contains_point(pos) {
                for rendered in &self.submenu_rendered_items {
                    if rendered.bounds.contains_point(pos) {
                        if let MenuItem::Action { action, enabled, .. } = &rendered.item {
                            if *enabled {
                                let action = action.clone();
                                self.hide();
                                return Some(action);
                            }
                        }
                    }
                }
                return None;
            }
        }

        // Check main menu click
        for rendered in &self.rendered_items {
            if rendered.bounds.contains_point(pos) {
                match &rendered.item {
                    MenuItem::Action { action, enabled, .. } => {
                        if *enabled {
                            let action = action.clone();
                            self.hide();
                            return Some(action);
                        }
                    }
                    MenuItem::Submenu { .. } => {
                        // Clicking on submenu parent opens it
                        if let Some(idx) = self.rendered_items.iter().position(|r| std::ptr::eq(r, rendered)) {
                            self.open_submenu(idx);
                        }
                        return None;
                    }
                    MenuItem::Separator => {}
                }
            }
        }

        // Click outside menu closes it
        if !self.menu_bounds.contains_point(pos) {
            if let Some(ref submenu_bounds) = self.submenu_bounds {
                if !submenu_bounds.contains_point(pos) {
                    self.hide();
                }
            } else {
                self.hide();
            }
        }

        None
    }

    /// Handle keyboard input. Returns action if selected.
    pub fn handle_key(&mut self, key: &Key) -> Option<ContextMenuAction> {
        if !self.visible {
            return None;
        }

        match key {
            Key::Escape => {
                // If submenu is open, close it; otherwise close menu
                if self.open_submenu_index.is_some() {
                    self.close_submenu();
                } else {
                    self.hide();
                }
                None
            }
            Key::Up => {
                if self.open_submenu_index.is_some() {
                    self.submenu_move_focus(-1);
                } else {
                    self.move_focus(-1);
                }
                None
            }
            Key::Down => {
                if self.open_submenu_index.is_some() {
                    self.submenu_move_focus(1);
                } else {
                    self.move_focus(1);
                }
                None
            }
            Key::Right => {
                // Open submenu if focused item is a submenu
                if let Some(idx) = self.focused_index {
                    if let Some(rendered) = self.rendered_items.get(idx) {
                        if matches!(&rendered.item, MenuItem::Submenu { .. }) {
                            self.open_submenu(idx);
                        }
                    }
                }
                None
            }
            Key::Left => {
                // Close submenu
                if self.open_submenu_index.is_some() {
                    self.close_submenu();
                }
                None
            }
            Key::Return => self.activate_focused(),
            _ => None,
        }
    }

    /// Move focus by delta (skipping separators).
    fn move_focus(&mut self, delta: i32) {
        let actionable_indices: Vec<usize> = self.rendered_items
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.item.is_separator())
            .map(|(i, _)| i)
            .collect();

        if actionable_indices.is_empty() {
            return;
        }

        let current_pos = self.focused_index
            .and_then(|idx| actionable_indices.iter().position(|&i| i == idx))
            .unwrap_or(0);

        let new_pos = if delta > 0 {
            (current_pos + 1) % actionable_indices.len()
        } else if current_pos == 0 {
            actionable_indices.len() - 1
        } else {
            current_pos - 1
        };

        self.focused_index = actionable_indices.get(new_pos).copied();
        self.hovered_index = self.focused_index;
    }

    /// Move submenu focus by delta.
    fn submenu_move_focus(&mut self, delta: i32) {
        let actionable_indices: Vec<usize> = self.submenu_rendered_items
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.item.is_separator())
            .map(|(i, _)| i)
            .collect();

        if actionable_indices.is_empty() {
            return;
        }

        let current_pos = self.submenu_focused_index
            .and_then(|idx| actionable_indices.iter().position(|&i| i == idx))
            .unwrap_or(0);

        let new_pos = if delta > 0 {
            (current_pos + 1) % actionable_indices.len()
        } else if current_pos == 0 {
            actionable_indices.len() - 1
        } else {
            current_pos - 1
        };

        self.submenu_focused_index = actionable_indices.get(new_pos).copied();
        self.submenu_hovered_index = self.submenu_focused_index;
    }

    /// Activate the focused item.
    fn activate_focused(&mut self) -> Option<ContextMenuAction> {
        // Check submenu first
        if self.open_submenu_index.is_some() {
            if let Some(item_idx) = self.submenu_focused_index {
                if let Some(rendered) = self.submenu_rendered_items.get(item_idx) {
                    if let MenuItem::Action { action, enabled, .. } = &rendered.item {
                        if *enabled {
                            let action = action.clone();
                            self.hide();
                            return Some(action);
                        }
                    }
                }
            }
            return None;
        }

        // Check main menu
        if let Some(idx) = self.focused_index {
            if let Some(rendered) = self.rendered_items.get(idx) {
                match &rendered.item {
                    MenuItem::Action { action, enabled, .. } => {
                        if *enabled {
                            let action = action.clone();
                            self.hide();
                            return Some(action);
                        }
                    }
                    MenuItem::Submenu { .. } => {
                        // Enter opens submenu
                        self.open_submenu(idx);
                    }
                    MenuItem::Separator => {}
                }
            }
        }

        None
    }

    /// Render the context menu.
    pub fn render(&self, renderer: &Renderer) -> Result<()> {
        if !self.visible {
            return Ok(());
        }

        let theme = renderer.theme();

        // Main menu background
        renderer.fill_rounded_rect(self.menu_bounds, 6.0, theme.background)?;
        renderer.stroke_rounded_rect(self.menu_bounds, 6.0, theme.border, 1.0)?;

        // Render main menu items
        for (i, rendered) in self.rendered_items.iter().enumerate() {
            let focused = self.focused_index == Some(i);
            let hovered = self.hovered_index == Some(i);
            self.render_item(renderer, rendered, focused, hovered)?;
        }

        // Render open submenu if any
        if self.open_submenu_index.is_some() {
            self.render_submenu(renderer)?;
        }

        Ok(())
    }

    /// Render a single menu item.
    fn render_item(&self, renderer: &Renderer, rendered: &RenderedItem, focused: bool, hovered: bool) -> Result<()> {
        let theme = renderer.theme();

        match &rendered.item {
            MenuItem::Separator => {
                let y = rendered.bounds.y + rendered.bounds.height as i32 / 2;
                renderer.line(
                    (rendered.bounds.x + 4) as f64,
                    y as f64,
                    (rendered.bounds.x + rendered.bounds.width as i32 - 4) as f64,
                    y as f64,
                    theme.border,
                    1.0,
                )?;
            }
            MenuItem::Action { label, shortcut, enabled, .. } => {
                // Background for hover/focus
                if (focused || hovered) && *enabled {
                    renderer.fill_rounded_rect(rendered.bounds, 4.0, theme.item_hover_background)?;
                }

                let text_color = if *enabled {
                    theme.foreground
                } else {
                    theme.item_description
                };

                let text_style = TextStyle::new()
                    .font_family(&theme.font_family)
                    .font_size(theme.font_size)
                    .color(text_color);

                // Label
                renderer.text(
                    label,
                    (rendered.bounds.x + 12) as f64,
                    (rendered.bounds.y + 6) as f64,
                    &text_style,
                )?;

                // Shortcut (right-aligned)
                if let Some(shortcut) = shortcut {
                    let shortcut_style = text_style.clone().color(theme.item_description);
                    let shortcut_width = renderer.measure_text(shortcut, &shortcut_style)
                        .map(|m| m.width as i32)
                        .unwrap_or(60);
                    let shortcut_x = rendered.bounds.x + rendered.bounds.width as i32 - shortcut_width - 12;
                    renderer.text(
                        shortcut,
                        shortcut_x as f64,
                        (rendered.bounds.y + 6) as f64,
                        &shortcut_style,
                    )?;
                }
            }
            MenuItem::Submenu { label, .. } => {
                // Background for hover/focus
                if focused || hovered {
                    renderer.fill_rounded_rect(rendered.bounds, 4.0, theme.item_hover_background)?;
                }

                let text_style = TextStyle::new()
                    .font_family(&theme.font_family)
                    .font_size(theme.font_size)
                    .color(theme.foreground);

                // Label
                renderer.text(
                    label,
                    (rendered.bounds.x + 12) as f64,
                    (rendered.bounds.y + 6) as f64,
                    &text_style,
                )?;

                // Arrow indicator (right-pointing)
                let arrow_x = rendered.bounds.x + rendered.bounds.width as i32 - 16;
                renderer.text(
                    ">",
                    arrow_x as f64,
                    (rendered.bounds.y + 6) as f64,
                    &text_style,
                )?;
            }
        }

        Ok(())
    }

    /// Render an open submenu.
    fn render_submenu(&self, renderer: &Renderer) -> Result<()> {
        let theme = renderer.theme();

        let submenu_bounds = match &self.submenu_bounds {
            Some(b) => *b,
            None => return Ok(()),
        };

        // Submenu background
        renderer.fill_rounded_rect(submenu_bounds, 6.0, theme.background)?;
        renderer.stroke_rounded_rect(submenu_bounds, 6.0, theme.border, 1.0)?;

        // Render submenu items
        for (i, rendered) in self.submenu_rendered_items.iter().enumerate() {
            let focused = self.submenu_focused_index == Some(i);
            let hovered = self.submenu_hovered_index == Some(i);
            self.render_item(renderer, rendered, focused, hovered)?;
        }

        Ok(())
    }
}
