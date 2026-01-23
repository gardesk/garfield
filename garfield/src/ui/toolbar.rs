//! Toolbar component with icon buttons.

use anyhow::Result;
use gartk_core::{Point, Rect, Theme};
use gartk_render::{Renderer, TextStyle};

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
    /// Show help modal.
    Help,
    /// Copy selected files.
    Copy,
    /// Cut selected files.
    Cut,
    /// Paste files from clipboard.
    Paste,
    /// Delete selected files to trash.
    Trash,
    /// Create new folder.
    NewFolder,
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
    has_selection: bool,
    has_clipboard: bool,
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
            has_selection: false,
            has_clipboard: false,
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

    /// Set file operation state.
    pub fn set_file_ops_state(&mut self, has_selection: bool, has_clipboard: bool) {
        self.has_selection = has_selection;
        self.has_clipboard = has_clipboard;
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

        x += GROUP_SEPARATOR as i32;

        // File operation buttons
        let file_buttons = [
            (ToolbarAction::Copy, "Copy (Ctrl+C)"),
            (ToolbarAction::Cut, "Cut (Ctrl+X)"),
            (ToolbarAction::Paste, "Paste (Ctrl+V)"),
            (ToolbarAction::Trash, "Delete (Del)"),
            (ToolbarAction::NewFolder, "New Folder (Ctrl+Shift+N)"),
        ];

        for (action, tooltip) in file_buttons {
            self.buttons.push(ToolbarButton {
                action,
                bounds: Rect::new(x, y, BUTTON_SIZE, BUTTON_SIZE),
                tooltip,
            });
            x += BUTTON_SIZE as i32 + BUTTON_PADDING as i32;
        }

        // Help button (right-aligned)
        let help_x = self.bounds.x + self.bounds.width as i32 - BUTTON_SIZE as i32 - BUTTON_PADDING as i32;
        self.buttons.push(ToolbarButton {
            action: ToolbarAction::Help,
            bounds: Rect::new(help_x, y, BUTTON_SIZE, BUTTON_SIZE),
            tooltip: "Help (F1)",
        });
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
                // Don't trigger disabled buttons
                match button.action {
                    ToolbarAction::GoBack if !self.can_go_back => return None,
                    ToolbarAction::GoForward if !self.can_go_forward => return None,
                    ToolbarAction::Copy | ToolbarAction::Cut | ToolbarAction::Trash
                        if !self.has_selection => return None,
                    ToolbarAction::Paste if !self.has_clipboard => return None,
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

    /// Render the toolbar (without tooltip - call render_tooltip_overlay separately).
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

    /// Render tooltip overlay (call after all other UI to ensure it's on top).
    pub fn render_tooltip_overlay(&self, renderer: &Renderer) -> Result<()> {
        if let Some(hovered_idx) = self.hovered {
            if let Some(button) = self.buttons.get(hovered_idx) {
                let theme = renderer.theme();
                self.render_tooltip(renderer, button, theme)?;
            }
        }
        Ok(())
    }

    /// Render tooltip for a button.
    fn render_tooltip(&self, renderer: &Renderer, button: &ToolbarButton, theme: &Theme) -> Result<()> {
        let text_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.foreground);

        let text_size = renderer.measure_text(button.tooltip, &text_style)?;

        let padding = 6;
        let tooltip_width = text_size.width + padding * 2;
        let tooltip_height = text_size.height + padding * 2;

        // Position tooltip below the button, centered
        let mut tooltip_x = button.bounds.x + (button.bounds.width as i32 - tooltip_width as i32) / 2;
        let tooltip_y = button.bounds.y + button.bounds.height as i32 + 4;

        // Clamp tooltip to stay within toolbar bounds (prevent right edge cutoff)
        let max_x = self.bounds.x + self.bounds.width as i32 - tooltip_width as i32 - 4;
        let min_x = self.bounds.x + 4;
        tooltip_x = tooltip_x.clamp(min_x, max_x);

        let tooltip_rect = Rect::new(tooltip_x, tooltip_y, tooltip_width, tooltip_height);

        // Draw tooltip background with solid dark color and visible border
        let bg_color = gartk_core::Color::from_u8(30, 30, 35, 255);
        let border_color = gartk_core::Color::from_u8(100, 100, 110, 255);
        renderer.fill_rounded_rect(tooltip_rect, 4.0, bg_color)?;
        renderer.stroke_rounded_rect(tooltip_rect, 4.0, border_color, 1.5)?;

        // Draw tooltip text
        renderer.text(
            button.tooltip,
            (tooltip_x + padding as i32) as f64,
            (tooltip_y + padding as i32) as f64,
            &text_style,
        )?;

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
            ToolbarAction::Copy | ToolbarAction::Cut | ToolbarAction::Trash => !self.has_selection,
            ToolbarAction::Paste => !self.has_clipboard,
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
            ToolbarAction::Help => self.draw_help_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::Copy => self.draw_copy_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::Cut => self.draw_cut_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::Paste => self.draw_paste_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::Trash => self.draw_trash_icon(renderer, cx, cy, icon_color)?,
            ToolbarAction::NewFolder => self.draw_new_folder_icon(renderer, cx, cy, icon_color)?,
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

    fn draw_help_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Draw circle outline
        let r = 9.0;
        let segments = 20;
        for i in 0..segments {
            let a1 = (i as f64 / segments as f64) * std::f64::consts::TAU;
            let a2 = ((i + 1) as f64 / segments as f64) * std::f64::consts::TAU;
            renderer.line(
                cx + r * a1.cos(),
                cy + r * a1.sin(),
                cx + r * a2.cos(),
                cy + r * a2.sin(),
                color,
                1.5,
            )?;
        }

        // Draw "?" shape inside
        // Top arc of question mark
        renderer.line(cx - 2.5, cy - 3.0, cx - 1.0, cy - 5.0, color, 1.5)?;
        renderer.line(cx - 1.0, cy - 5.0, cx + 2.0, cy - 5.0, color, 1.5)?;
        renderer.line(cx + 2.0, cy - 5.0, cx + 3.0, cy - 3.0, color, 1.5)?;
        // Curve down to stem
        renderer.line(cx + 3.0, cy - 3.0, cx + 1.0, cy - 1.0, color, 1.5)?;
        renderer.line(cx + 1.0, cy - 1.0, cx, cy + 1.0, color, 1.5)?;
        // Dot at bottom
        renderer.fill_rect(Rect::new((cx - 1.0) as i32, (cy + 3.0) as i32, 3, 3), color)?;
        Ok(())
    }

    fn draw_copy_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Two overlapping rectangles (copy symbol)
        let w = 7.0;
        let h = 9.0;
        let offset = 3.0;

        // Back rectangle (slightly offset)
        let bx = cx - w/2.0 - offset/2.0;
        let by = cy - h/2.0 - offset/2.0;
        renderer.stroke_rect(Rect::new(bx as i32, by as i32, w as u32, h as u32), color, 1.5)?;

        // Front rectangle
        let fx = cx - w/2.0 + offset/2.0;
        let fy = cy - h/2.0 + offset/2.0;
        renderer.fill_rect(Rect::new(fx as i32, fy as i32, w as u32, h as u32), color.with_alpha(0.3))?;
        renderer.stroke_rect(Rect::new(fx as i32, fy as i32, w as u32, h as u32), color, 1.5)?;
        Ok(())
    }

    fn draw_cut_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Scissors shape - two circles with crossed lines
        let r = 3.0;
        let segments = 12;

        // Left circle (handle)
        let lcx = cx - 4.0;
        let lcy = cy + 3.0;
        for i in 0..segments {
            let a1 = (i as f64 / segments as f64) * std::f64::consts::TAU;
            let a2 = ((i + 1) as f64 / segments as f64) * std::f64::consts::TAU;
            renderer.line(
                lcx + r * a1.cos(), lcy + r * a1.sin(),
                lcx + r * a2.cos(), lcy + r * a2.sin(),
                color, 1.5
            )?;
        }

        // Right circle (handle)
        let rcx = cx + 4.0;
        let rcy = cy + 3.0;
        for i in 0..segments {
            let a1 = (i as f64 / segments as f64) * std::f64::consts::TAU;
            let a2 = ((i + 1) as f64 / segments as f64) * std::f64::consts::TAU;
            renderer.line(
                rcx + r * a1.cos(), rcy + r * a1.sin(),
                rcx + r * a2.cos(), rcy + r * a2.sin(),
                color, 1.5
            )?;
        }

        // Blades (crossed lines going up)
        renderer.line(lcx, lcy - r, cx + 2.0, cy - 6.0, color, 1.5)?;
        renderer.line(rcx, rcy - r, cx - 2.0, cy - 6.0, color, 1.5)?;
        Ok(())
    }

    fn draw_paste_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Clipboard with paper
        let w = 10.0;
        let h = 12.0;

        // Clipboard outline
        let bx = cx - w/2.0;
        let by = cy - h/2.0 + 1.0;
        renderer.stroke_rect(Rect::new(bx as i32, by as i32, w as u32, h as u32), color, 1.5)?;

        // Clip at top (small rectangle)
        let clip_w = 5.0;
        let clip_h = 3.0;
        let clip_x = cx - clip_w/2.0;
        let clip_y = by - clip_h/2.0;
        renderer.fill_rect(Rect::new(clip_x as i32, clip_y as i32, clip_w as u32, clip_h as u32), color)?;

        // Lines on clipboard (document content)
        let line_y1 = by + 4.0;
        let line_y2 = by + 7.0;
        renderer.line(bx + 2.0, line_y1, bx + w - 2.0, line_y1, color, 1.5)?;
        renderer.line(bx + 2.0, line_y2, bx + w - 2.0, line_y2, color, 1.5)?;
        Ok(())
    }

    fn draw_trash_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Trash can shape
        let w = 10.0;
        let h = 10.0;

        // Body (trapezoid-ish)
        let bx = cx - w/2.0;
        let by = cy - h/2.0 + 2.0;
        let bw = w;
        let bh = h - 2.0;
        renderer.line(bx, by, bx + 1.0, by + bh, color, 1.5)?;
        renderer.line(bx + 1.0, by + bh, bx + bw - 1.0, by + bh, color, 1.5)?;
        renderer.line(bx + bw - 1.0, by + bh, bx + bw, by, color, 1.5)?;

        // Lid
        let lid_y = cy - h/2.0;
        renderer.line(cx - w/2.0 - 1.0, lid_y, cx + w/2.0 + 1.0, lid_y, color, 2.0)?;

        // Handle on lid
        renderer.line(cx - 2.0, lid_y, cx - 2.0, lid_y - 2.0, color, 1.5)?;
        renderer.line(cx - 2.0, lid_y - 2.0, cx + 2.0, lid_y - 2.0, color, 1.5)?;
        renderer.line(cx + 2.0, lid_y - 2.0, cx + 2.0, lid_y, color, 1.5)?;

        // Vertical lines inside
        renderer.line(cx - 2.0, by + 2.0, cx - 2.0, by + bh - 2.0, color, 1.0)?;
        renderer.line(cx, by + 2.0, cx, by + bh - 2.0, color, 1.0)?;
        renderer.line(cx + 2.0, by + 2.0, cx + 2.0, by + bh - 2.0, color, 1.0)?;
        Ok(())
    }

    fn draw_new_folder_icon(&self, renderer: &Renderer, cx: f64, cy: f64, color: gartk_core::Color) -> Result<()> {
        // Folder shape with plus sign
        let w = 12.0;
        let h = 9.0;
        let tab_w = 5.0;
        let tab_h = 2.0;

        let fx = cx - w/2.0;
        let fy = cy - h/2.0;

        // Folder tab
        renderer.line(fx, fy + tab_h, fx, fy, color, 1.5)?;
        renderer.line(fx, fy, fx + tab_w, fy, color, 1.5)?;
        renderer.line(fx + tab_w, fy, fx + tab_w + 2.0, fy + tab_h, color, 1.5)?;

        // Folder body
        renderer.line(fx + tab_w + 2.0, fy + tab_h, fx + w, fy + tab_h, color, 1.5)?;
        renderer.line(fx + w, fy + tab_h, fx + w, fy + h, color, 1.5)?;
        renderer.line(fx + w, fy + h, fx, fy + h, color, 1.5)?;
        renderer.line(fx, fy + h, fx, fy + tab_h, color, 1.5)?;

        // Plus sign in center
        let plus_size = 4.0;
        let pcy = cy + 1.0;
        renderer.line(cx - plus_size/2.0, pcy, cx + plus_size/2.0, pcy, color, 1.5)?;
        renderer.line(cx, pcy - plus_size/2.0, cx, pcy + plus_size/2.0, color, 1.5)?;
        Ok(())
    }
}
