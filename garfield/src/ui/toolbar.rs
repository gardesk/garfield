//! Toolbar component with icon buttons.

use anyhow::Result;
use gartk_core::{Point, Rect, Theme};
use gartk_render::Renderer;

/// Height of the toolbar.
pub const TOOLBAR_HEIGHT: u32 = 36;

/// Size of toolbar buttons.
const BUTTON_SIZE: u32 = 28;

/// Padding between buttons.
const BUTTON_PADDING: u32 = 4;

/// Button groups separator width.
const GROUP_SEPARATOR: u32 = 12;

/// A toolbar action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    /// Switch to list view.
    ViewList,
    /// Switch to grid view.
    ViewGrid,
    /// Switch to column view.
    ViewColumns,
    /// Create new tab.
    NewTab,
    /// Split pane horizontally.
    SplitHorizontal,
    /// Split pane vertically.
    SplitVertical,
    /// Go back in history.
    GoBack,
    /// Go forward in history.
    GoForward,
    /// Go up to parent directory.
    GoUp,
}

/// A toolbar button.
#[derive(Debug)]
struct ToolbarButton {
    action: ToolbarAction,
    bounds: Rect,
    tooltip: &'static str,
}

/// Toolbar component.
pub struct Toolbar {
    bounds: Rect,
    buttons: Vec<ToolbarButton>,
    hovered: Option<usize>,
    active_view: ToolbarAction,
    can_go_back: bool,
    can_go_forward: bool,
}

impl Toolbar {
    /// Create a new toolbar.
    pub fn new(bounds: Rect) -> Self {
        let mut toolbar = Self {
            bounds,
            buttons: Vec::new(),
            hovered: None,
            active_view: ToolbarAction::ViewList,
            can_go_back: false,
            can_go_forward: false,
        };
        toolbar.layout_buttons();
        toolbar
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.layout_buttons();
    }

    /// Set active view mode.
    pub fn set_active_view(&mut self, action: ToolbarAction) {
        self.active_view = action;
    }

    /// Set navigation state.
    pub fn set_nav_state(&mut self, can_back: bool, can_forward: bool) {
        self.can_go_back = can_back;
        self.can_go_forward = can_forward;
    }

    /// Layout buttons.
    fn layout_buttons(&mut self) {
        self.buttons.clear();

        let y = self.bounds.y + (self.bounds.height as i32 - BUTTON_SIZE as i32) / 2;
        let mut x = self.bounds.x + BUTTON_PADDING as i32;

        // Navigation buttons
        let nav_buttons = [
            (ToolbarAction::GoBack, "Go Back (Alt+Left)"),
            (ToolbarAction::GoForward, "Go Forward (Alt+Right)"),
            (ToolbarAction::GoUp, "Go Up (Backspace)"),
        ];

        for (action, tooltip) in nav_buttons {
            self.buttons.push(ToolbarButton {
                action,
                bounds: Rect::new(x, y, BUTTON_SIZE, BUTTON_SIZE),
                tooltip,
            });
            x += BUTTON_SIZE as i32 + BUTTON_PADDING as i32;
        }

        x += GROUP_SEPARATOR as i32;

        // View mode buttons
        let view_buttons = [
            (ToolbarAction::ViewList, "List View (Ctrl+1)"),
            (ToolbarAction::ViewGrid, "Grid View (Ctrl+2)"),
            (ToolbarAction::ViewColumns, "Column View (Ctrl+3)"),
        ];

        for (action, tooltip) in view_buttons {
            self.buttons.push(ToolbarButton {
                action,
                bounds: Rect::new(x, y, BUTTON_SIZE, BUTTON_SIZE),
                tooltip,
            });
            x += BUTTON_SIZE as i32 + BUTTON_PADDING as i32;
        }

        x += GROUP_SEPARATOR as i32;

        // Tab/pane buttons
        let pane_buttons = [
            (ToolbarAction::NewTab, "New Tab (Ctrl+T)"),
            (ToolbarAction::SplitHorizontal, "Split Horizontal (Ctrl+Shift+H)"),
            (ToolbarAction::SplitVertical, "Split Vertical (Ctrl+Shift+V)"),
        ];

        for (action, tooltip) in pane_buttons {
            self.buttons.push(ToolbarButton {
                action,
                bounds: Rect::new(x, y, BUTTON_SIZE, BUTTON_SIZE),
                tooltip,
            });
            x += BUTTON_SIZE as i32 + BUTTON_PADDING as i32;
        }
    }

    /// Handle mouse move.
    pub fn on_mouse_move(&mut self, pos: Point) {
        self.hovered = self.buttons.iter().position(|b| b.bounds.contains_point(pos));
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered = None;
    }

    /// Handle click. Returns action if a button was clicked.
    pub fn on_click(&self, pos: Point) -> Option<ToolbarAction> {
        for button in &self.buttons {
            if button.bounds.contains_point(pos) {
                // Don't trigger disabled nav buttons
                match button.action {
                    ToolbarAction::GoBack if !self.can_go_back => return None,
                    ToolbarAction::GoForward if !self.can_go_forward => return None,
                    _ => return Some(button.action),
                }
            }
        }
        None
    }

    /// Check if point is within toolbar bounds.
    pub fn contains(&self, pos: Point) -> bool {
        self.bounds.contains_point(pos)
    }

    /// Get hovered button tooltip.
    pub fn hovered_tooltip(&self) -> Option<&'static str> {
        self.hovered.map(|i| self.buttons[i].tooltip)
    }

    /// Render the toolbar.
    pub fn render(&self, renderer: &Renderer) -> Result<()> {
        let theme = renderer.theme();

        // Draw toolbar background
        renderer.fill_rect(self.bounds, theme.background)?;

        // Draw bottom border
        renderer.line(
            self.bounds.x as f64,
            (self.bounds.y + self.bounds.height as i32) as f64,
            (self.bounds.x + self.bounds.width as i32) as f64,
            (self.bounds.y + self.bounds.height as i32) as f64,
            theme.border,
            1.0,
        )?;

        // Draw buttons
        for (i, button) in self.buttons.iter().enumerate() {
            self.render_button(renderer, button, i, theme)?;
        }

        Ok(())
    }

    /// Render a single button.
    fn render_button(&self, renderer: &Renderer, button: &ToolbarButton, index: usize, theme: &Theme) -> Result<()> {
        let is_hovered = self.hovered == Some(index);
        let is_active = match button.action {
            ToolbarAction::ViewList | ToolbarAction::ViewGrid | ToolbarAction::ViewColumns => {
                button.action == self.active_view
            }
            _ => false,
        };
        let is_disabled = match button.action {
            ToolbarAction::GoBack => !self.can_go_back,
            ToolbarAction::GoForward => !self.can_go_forward,
            _ => false,
        };

        // Button background
        let bg_color = if is_active {
            theme.selection_background
        } else if is_hovered && !is_disabled {
            theme.item_hover_background
        } else {
            theme.background
        };

        renderer.fill_rounded_rect(button.bounds, 4.0, bg_color)?;

        // Icon color
        let icon_color = if is_disabled {
            theme.item_description
        } else if is_active {
            theme.selection_foreground
        } else {
            theme.foreground
        };

        // Draw icon
        let cx = button.bounds.x as f64 + button.bounds.width as f64 / 2.0;
        let cy = button.bounds.y as f64 + button.bounds.height as f64 / 2.0;

        match button.action {
            ToolbarAction::GoBack => self.draw_back_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::GoForward => self.draw_forward_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::GoUp => self.draw_up_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::ViewList => self.draw_list_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::ViewGrid => self.draw_grid_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::ViewColumns => self.draw_columns_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::NewTab => self.draw_new_tab_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::SplitHorizontal => self.draw_split_h_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::SplitVertical => self.draw_split_v_icon(renderer, cx, cy, icon_color)?,
        }

        Ok(())
    }

    // === Icon drawing functions ===

    fn draw_back_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Left arrow
        let size = 8.0;
        renderer.line(cx + size/2.0, cy - size/2.0, cx - size/2.0, cy, color, 2.0)?;
        renderer.line(cx - size/2.0, cy, cx + size/2.0, cy + size/2.0, color, 2.0)?;
        Ok(())
    }

    fn draw_forward_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Right arrow
        let size = 8.0;
        renderer.line(cx - size/2.0, cy - size/2.0, cx + size/2.0, cy, color, 2.0)?;
        renderer.line(cx + size/2.0, cy, cx - size/2.0, cy + size/2.0, color, 2.0)?;
        Ok(())
    }

    fn draw_up_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Up arrow
        let size = 8.0;
        renderer.line(cx - size/2.0, cy + size/4.0, cx, cy - size/2.0, color, 2.0)?;
        renderer.line(cx, cy - size/2.0, cx + size/2.0, cy + size/4.0, color, 2.0)?;
        renderer.line(cx, cy - size/4.0, cx, cy + size/2.0, color, 2.0)?;
        Ok(())
    }

    fn draw_list_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Three horizontal lines
        let w = 10.0;
        let h = 8.0;
        for i in 0..3 {
            let y = cy - h/2.0 + (i as f64) * (h/2.0);
            renderer.line(cx - w/2.0, y, cx + w/2.0, y, color, 2.0)?;
        }
        Ok(())
    }

    fn draw_grid_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // 2x2 grid of squares
        let size = 4.0;
        let gap = 2.0;
        for row in 0..2 {
            for col in 0..2 {
                let x = cx - size - gap/2.0 + (col as f64) * (size + gap);
                let y = cy - size - gap/2.0 + (row as f64) * (size + gap);
                let rect = Rect::new(x as i32, y as i32, size as u32, size as u32);
                renderer.fill_rect(rect, color)?;
            }
        }
        Ok(())
    }

    fn draw_columns_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Three vertical rectangles (Miller columns)
        let h = 10.0;
        let w = 3.0;
        let gap = 2.0;
        for i in 0..3 {
            let x = cx - (w * 1.5 + gap) + (i as f64) * (w + gap);
            let rect = Rect::new(x as i32, (cy - h/2.0) as i32, w as u32, h as u32);
            renderer.fill_rect(rect, color)?;
        }
        Ok(())
    }

    fn draw_new_tab_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Plus sign
        let size = 8.0;
        renderer.line(cx - size/2.0, cy, cx + size/2.0, cy, color, 2.0)?;
        renderer.line(cx, cy - size/2.0, cx, cy + size/2.0, color, 2.0)?;
        Ok(())
    }

    fn draw_split_h_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Rectangle with vertical line in middle
        let w = 12.0;
        let h = 8.0;
        // Outer rect
        renderer.line(cx - w/2.0, cy - h/2.0, cx + w/2.0, cy - h/2.0, color, 1.5)?;
        renderer.line(cx + w/2.0, cy - h/2.0, cx + w/2.0, cy + h/2.0, color, 1.5)?;
        renderer.line(cx + w/2.0, cy + h/2.0, cx - w/2.0, cy + h/2.0, color, 1.5)?;
        renderer.line(cx - w/2.0, cy + h/2.0, cx - w/2.0, cy - h/2.0, color, 1.5)?;
        // Vertical divider
        renderer.line(cx, cy - h/2.0, cx, cy + h/2.0, color, 1.5)?;
        Ok(())
    }

    fn draw_split_v_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Rectangle with horizontal line in middle
        let w = 12.0;
        let h = 8.0;
        // Outer rect
        renderer.line(cx - w/2.0, cy - h/2.0, cx + w/2.0, cy - h/2.0, color, 1.5)?;
        renderer.line(cx + w/2.0, cy - h/2.0, cx + w/2.0, cy + h/2.0, color, 1.5)?;
        renderer.line(cx + w/2.0, cy + h/2.0, cx - w/2.0, cy + h/2.0, color, 1.5)?;
        renderer.line(cx - w/2.0, cy + h/2.0, cx - w/2.0, cy - h/2.0, color, 1.5)?;
        // Horizontal divider
        renderer.line(cx - w/2.0, cy, cx + w/2.0, cy, color, 1.5)?;
        Ok(())
    }
}
