//! Tab bar component for displaying and managing tabs.

use gartk_core::{Point, Rect};
use gartk_render::{Renderer, TextStyle};

/// Height of the tab bar.
pub const TAB_BAR_HEIGHT: u32 = 32;

/// Width of the close button area.
const CLOSE_BUTTON_WIDTH: u32 = 20;

/// Minimum tab width.
const MIN_TAB_WIDTH: u32 = 80;

/// Maximum tab width.
const MAX_TAB_WIDTH: u32 = 200;

/// Padding inside tabs.
const TAB_PADDING: u32 = 12;

/// Information about a tab for rendering.
#[derive(Clone)]
pub struct TabInfo {
    /// Tab title.
    pub title: String,
    /// Whether this tab is active.
    pub active: bool,
}

/// Tab bar for displaying tabs.
pub struct TabBar {
    /// Component bounds.
    bounds: Rect,
    /// Tab information (titles and active state).
    tabs: Vec<TabInfo>,
    /// Active tab index.
    active_index: usize,
    /// Hovered tab index.
    hovered_tab: Option<usize>,
    /// Hovered close button index.
    hovered_close: Option<usize>,
    /// Cached tab bounds.
    tab_bounds: Vec<Rect>,
}

impl TabBar {
    /// Create a new tab bar.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            tabs: Vec::new(),
            active_index: 0,
            hovered_tab: None,
            hovered_close: None,
            tab_bounds: Vec::new(),
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.recalculate_tab_bounds();
    }

    /// Set the tabs to display.
    pub fn set_tabs(&mut self, tabs: Vec<TabInfo>, active_index: usize) {
        self.tabs = tabs;
        self.active_index = active_index;
        self.recalculate_tab_bounds();
    }

    /// Update only the active index (more efficient than set_tabs).
    pub fn set_active(&mut self, index: usize) {
        self.active_index = index;
    }

    /// Get tab count.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Recalculate tab bounds based on number of tabs.
    fn recalculate_tab_bounds(&mut self) {
        self.tab_bounds.clear();

        if self.tabs.is_empty() {
            return;
        }

        let available_width = self.bounds.width;
        let tab_count = self.tabs.len() as u32;

        // Calculate tab width (equal distribution, clamped)
        let raw_width = available_width / tab_count;
        let tab_width = raw_width.clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH);

        let mut x = self.bounds.x;
        for _ in 0..self.tabs.len() {
            self.tab_bounds.push(Rect::new(x, self.bounds.y, tab_width, TAB_BAR_HEIGHT));
            x += tab_width as i32;
        }
    }

    /// Get the close button bounds for a tab.
    fn close_button_bounds(&self, tab_index: usize) -> Option<Rect> {
        self.tab_bounds.get(tab_index).map(|tab| {
            Rect::new(
                tab.x + tab.width as i32 - CLOSE_BUTTON_WIDTH as i32 - 4,
                tab.y + 6,
                CLOSE_BUTTON_WIDTH,
                TAB_BAR_HEIGHT - 12,
            )
        })
    }

    /// Handle mouse move for hover effects.
    pub fn on_mouse_move(&mut self, pos: Point) {
        self.hovered_tab = None;
        self.hovered_close = None;

        if !self.bounds.contains_point(pos) {
            return;
        }

        for (i, tab_bounds) in self.tab_bounds.iter().enumerate() {
            if tab_bounds.contains_point(pos) {
                // Check if hovering close button
                if let Some(close_bounds) = self.close_button_bounds(i) {
                    if close_bounds.contains_point(pos) {
                        self.hovered_close = Some(i);
                        return;
                    }
                }
                self.hovered_tab = Some(i);
                return;
            }
        }
    }

    /// Handle click. Returns (clicked_tab, is_close_button).
    pub fn on_click(&self, pos: Point) -> Option<(usize, bool)> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        for (i, tab_bounds) in self.tab_bounds.iter().enumerate() {
            if tab_bounds.contains_point(pos) {
                // Check if clicking close button
                if let Some(close_bounds) = self.close_button_bounds(i) {
                    if close_bounds.contains_point(pos) {
                        return Some((i, true));
                    }
                }
                return Some((i, false));
            }
        }

        None
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered_tab = None;
        self.hovered_close = None;
    }

    /// Render the tab bar.
    pub fn render(&self, renderer: &Renderer) -> anyhow::Result<()> {
        let theme = renderer.theme();

        // Draw background
        renderer.fill_rect(self.bounds, theme.item_background.darken(0.05))?;

        // Draw bottom border
        renderer.line(
            self.bounds.x as f64,
            (self.bounds.y + self.bounds.height as i32) as f64,
            (self.bounds.x + self.bounds.width as i32) as f64,
            (self.bounds.y + self.bounds.height as i32) as f64,
            theme.border,
            1.0,
        )?;

        // Draw tabs
        for (i, (tab, bounds)) in self.tabs.iter().zip(self.tab_bounds.iter()).enumerate() {
            let is_active = i == self.active_index;
            let is_hovered = self.hovered_tab == Some(i);

            // Tab background
            if is_active {
                renderer.fill_rect(*bounds, theme.background)?;
                // Remove bottom border for active tab (visual connection to content)
                renderer.line(
                    bounds.x as f64,
                    (bounds.y + bounds.height as i32) as f64,
                    (bounds.x + bounds.width as i32) as f64,
                    (bounds.y + bounds.height as i32) as f64,
                    theme.background,
                    1.0,
                )?;
            } else if is_hovered {
                renderer.fill_rect(*bounds, theme.item_background)?;
            }

            // Tab title
            let text_color = if is_active {
                theme.foreground
            } else {
                theme.foreground.with_alpha(0.7)
            };

            let text_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size - 1.0)
                .color(text_color);

            // Truncate title if needed
            let max_title_width = bounds.width.saturating_sub(TAB_PADDING * 2 + CLOSE_BUTTON_WIDTH);
            let max_chars = (max_title_width / 8) as usize;
            let display_title = if tab.title.len() > max_chars && max_chars > 3 {
                format!("{}...", &tab.title[..max_chars - 3])
            } else {
                tab.title.clone()
            };

            let text_rect = Rect::new(
                bounds.x + TAB_PADDING as i32,
                bounds.y,
                max_title_width,
                TAB_BAR_HEIGHT,
            );
            renderer.text_in_rect(&display_title, text_rect, &text_style)?;

            // Close button (only if more than one tab or always show)
            if self.tabs.len() > 1 {
                if let Some(close_bounds) = self.close_button_bounds(i) {
                    let close_hovered = self.hovered_close == Some(i);

                    if close_hovered {
                        renderer.fill_rounded_rect(
                            close_bounds,
                            4.0,
                            theme.item_selected_background.with_alpha(0.5),
                        )?;
                    }

                    let close_color = if close_hovered {
                        theme.foreground
                    } else {
                        theme.foreground.with_alpha(0.5)
                    };

                    // Draw X
                    let cx = close_bounds.x + close_bounds.width as i32 / 2;
                    let cy = close_bounds.y + close_bounds.height as i32 / 2;
                    let size = 4;
                    renderer.line(
                        (cx - size) as f64,
                        (cy - size) as f64,
                        (cx + size) as f64,
                        (cy + size) as f64,
                        close_color,
                        1.5,
                    )?;
                    renderer.line(
                        (cx + size) as f64,
                        (cy - size) as f64,
                        (cx - size) as f64,
                        (cy + size) as f64,
                        close_color,
                        1.5,
                    )?;
                }
            }

            // Tab separator (right edge)
            if i < self.tabs.len() - 1 && !is_active && self.active_index != i + 1 {
                renderer.line(
                    (bounds.x + bounds.width as i32) as f64,
                    (bounds.y + 8) as f64,
                    (bounds.x + bounds.width as i32) as f64,
                    (bounds.y + bounds.height as i32 - 8) as f64,
                    theme.border.with_alpha(0.3),
                    1.0,
                )?;
            }
        }

        Ok(())
    }
}
