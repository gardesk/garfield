//! Breadcrumb path bar with clickable segments.

use gartk_core::{Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::path::{Path, PathBuf};

/// A single breadcrumb segment.
#[derive(Debug, Clone)]
struct Segment {
    /// Display text.
    text: String,
    /// Full path up to and including this segment.
    path: PathBuf,
    /// Bounding rectangle for hit testing.
    bounds: Rect,
}

/// Breadcrumb path bar.
pub struct Breadcrumb {
    /// Path segments.
    segments: Vec<Segment>,
    /// Component bounds.
    bounds: Rect,
    /// Hovered segment index.
    hovered: Option<usize>,
    /// Separator string.
    separator: String,
}

impl Breadcrumb {
    /// Create a new breadcrumb bar.
    pub fn new(bounds: Rect) -> Self {
        Self {
            segments: Vec::new(),
            bounds,
            hovered: None,
            separator: " / ".to_string(),
        }
    }

    /// Update the path displayed.
    pub fn set_path(&mut self, path: &Path) {
        self.segments.clear();
        self.hovered = None;

        let mut accumulated = PathBuf::new();

        // Add each path component
        for component in path.components() {
            use std::path::Component;
            match component {
                Component::RootDir => {
                    accumulated.push("/");
                    // Root is shown as "/" - no separator before it
                    self.segments.push(Segment {
                        text: "/".to_string(),
                        path: accumulated.clone(),
                        bounds: Rect::new(0, 0, 0, 0),
                    });
                }
                Component::Normal(name) => {
                    accumulated.push(name);
                    self.segments.push(Segment {
                        text: name.to_string_lossy().to_string(),
                        path: accumulated.clone(),
                        bounds: Rect::new(0, 0, 0, 0),
                    });
                }
                Component::ParentDir => {
                    accumulated.push("..");
                    self.segments.push(Segment {
                        text: "..".to_string(),
                        path: accumulated.clone(),
                        bounds: Rect::new(0, 0, 0, 0),
                    });
                }
                _ => {}
            }
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Handle mouse move for hover effects. Returns true if hover state changed.
    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        let old_hovered = self.hovered;

        if !self.bounds.contains_point(pos) {
            self.hovered = None;
            return self.hovered != old_hovered;
        }

        self.hovered = self.segments.iter().position(|s| s.bounds.contains_point(pos));
        self.hovered != old_hovered
    }

    /// Handle mouse click. Returns the path to navigate to, if any.
    pub fn on_click(&self, pos: Point) -> Option<PathBuf> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        for segment in &self.segments {
            if segment.bounds.contains_point(pos) {
                return Some(segment.path.clone());
            }
        }

        None
    }

    /// Get segment path and bounds at a point (for drag target detection).
    pub fn segment_at_point(&self, pos: Point) -> Option<(PathBuf, Rect)> {
        if !self.bounds.contains_point(pos) {
            return None;
        }

        for segment in &self.segments {
            if segment.bounds.contains_point(pos) {
                return Some((segment.path.clone(), segment.bounds));
            }
        }

        None
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered = None;
    }

    /// Render the breadcrumb bar.
    pub fn render(&mut self, renderer: &Renderer, can_go_back: bool, can_go_forward: bool) -> anyhow::Result<()> {
        let theme = renderer.theme();

        // Draw background
        renderer.fill_rect(self.bounds, theme.item_background)?;

        // Calculate text style
        let style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 1.0)
            .color(theme.item_foreground);

        let hover_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 1.0)
            .color(theme.selection_background);

        let separator_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 1.0)
            .color(theme.item_foreground.with_alpha(0.5));

        // Draw back/forward buttons
        let button_width = 24;
        let button_y = self.bounds.y + (self.bounds.height as i32 - 20) / 2;

        // Back button
        let back_color = if can_go_back {
            theme.item_foreground
        } else {
            theme.item_foreground.with_alpha(0.3)
        };
        let back_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 2.0)
            .color(back_color);
        renderer.text("<", (self.bounds.x + 8) as f64, button_y as f64, &back_style)?;

        // Forward button
        let forward_color = if can_go_forward {
            theme.item_foreground
        } else {
            theme.item_foreground.with_alpha(0.3)
        };
        let forward_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 2.0)
            .color(forward_color);
        renderer.text(">", (self.bounds.x + 8 + button_width) as f64, button_y as f64, &forward_style)?;

        // Start position for path segments (after buttons)
        let start_x = self.bounds.x + 8 + button_width * 2 + 8;
        let text_y = self.bounds.y + (self.bounds.height as i32 - theme.font_size as i32) / 2;
        let available_width = (self.bounds.x + self.bounds.width as i32 - start_x - 16) as u32;

        // Check if first segment is root "/" for separator logic
        let first_is_root = self.segments.first().map(|s| s.text == "/").unwrap_or(false);

        // Measure total width and individual segment widths
        let ellipsis = "...";
        let ellipsis_size = renderer.measure_text(ellipsis, &separator_style)?;
        let sep_size = renderer.measure_text(&self.separator, &separator_style)?;

        let mut segment_widths: Vec<u32> = Vec::new();
        let mut total_width: u32 = 0;

        for (i, segment) in self.segments.iter().enumerate() {
            let seg_width = renderer.measure_text(&segment.text, &style)?.width + 4;
            segment_widths.push(seg_width);

            // Add separator width
            if i > 0 {
                let sep_w = if i == 1 && first_is_root { 8 } else { sep_size.width };
                total_width += sep_w;
            }
            total_width += seg_width;
        }

        // Determine which segments to skip (truncate from left)
        let mut skip_count = 0;
        let mut show_ellipsis = false;

        if total_width > available_width && self.segments.len() > 2 {
            // Need to truncate - always show at least root and last segment
            let mut running_width = ellipsis_size.width + sep_size.width; // "... / "

            // Start from the end and work backwards to find how many we can show
            let mut can_show_from = self.segments.len();
            for i in (1..self.segments.len()).rev() {
                let seg_width = segment_widths[i] + sep_size.width;
                if running_width + seg_width <= available_width {
                    running_width += seg_width;
                    can_show_from = i;
                } else {
                    break;
                }
            }

            if can_show_from > 1 {
                skip_count = can_show_from - 1; // Skip segments 1 to can_show_from-1 (keep root)
                show_ellipsis = true;
            }
        }

        // Render segments
        let mut x = start_x;

        for (i, segment) in self.segments.iter_mut().enumerate() {
            // Skip truncated segments (but always show root at index 0)
            if i > 0 && i <= skip_count {
                // Clear bounds for skipped segments
                segment.bounds = Rect::new(0, 0, 0, 0);
                continue;
            }

            // Add ellipsis after root if truncating
            if show_ellipsis && i == skip_count + 1 {
                let sep = if first_is_root { " " } else { &self.separator };
                let sep_w = renderer.measure_text(sep, &separator_style)?;
                renderer.text(sep, x as f64, text_y as f64, &separator_style)?;
                x += sep_w.width as i32;

                renderer.text(ellipsis, x as f64, text_y as f64, &separator_style)?;
                x += ellipsis_size.width as i32;

                renderer.text(&self.separator, x as f64, text_y as f64, &separator_style)?;
                x += sep_size.width as i32;
            } else if i > 0 && !(show_ellipsis && i == skip_count + 1) {
                // Normal separator
                let sep = if i == 1 && first_is_root {
                    " "
                } else {
                    &self.separator
                };
                let sep_w = renderer.measure_text(sep, &separator_style)?;
                renderer.text(sep, x as f64, text_y as f64, &separator_style)?;
                x += sep_w.width as i32;
            }

            // Measure segment
            let text_style = if self.hovered == Some(i) {
                &hover_style
            } else {
                &style
            };
            let size = renderer.measure_text(&segment.text, text_style)?;

            // Update bounds for hit testing
            segment.bounds = Rect::new(x, self.bounds.y, size.width + 4, self.bounds.height);

            // Draw underline on hover
            if self.hovered == Some(i) {
                renderer.line(
                    x as f64,
                    (text_y + size.height as i32 + 2) as f64,
                    (x + size.width as i32) as f64,
                    (text_y + size.height as i32 + 2) as f64,
                    theme.selection_background,
                    1.0,
                )?;
            }

            // Draw text
            renderer.text(&segment.text, x as f64, text_y as f64, text_style)?;

            x += size.width as i32 + 4;
        }

        Ok(())
    }

    /// Get back button bounds for hit testing.
    pub fn back_button_bounds(&self) -> Rect {
        Rect::new(self.bounds.x, self.bounds.y, 24, self.bounds.height)
    }

    /// Get forward button bounds for hit testing.
    pub fn forward_button_bounds(&self) -> Rect {
        Rect::new(self.bounds.x + 24, self.bounds.y, 24, self.bounds.height)
    }
}
