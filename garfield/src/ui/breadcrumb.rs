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

    /// Handle mouse move for hover effects.
    pub fn on_mouse_move(&mut self, pos: Point) {
        if !self.bounds.contains_point(pos) {
            self.hovered = None;
            return;
        }

        self.hovered = self.segments.iter().position(|s| s.bounds.contains_point(pos));
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
        let mut x = self.bounds.x + 8 + button_width * 2 + 8;
        let text_y = self.bounds.y + (self.bounds.height as i32 - theme.font_size as i32) / 2;

        // Check if first segment is root "/" for separator logic
        let first_is_root = self.segments.first().map(|s| s.text == "/").unwrap_or(false);

        // Measure and render segments
        for (i, segment) in self.segments.iter_mut().enumerate() {
            // Add separator before non-root segments (but not after root "/")
            if i > 0 {
                // If previous segment was root "/", just add space, not " / "
                let sep = if i == 1 && first_is_root {
                    " "
                } else {
                    &self.separator
                };
                let sep_size = renderer.measure_text(sep, &separator_style)?;
                renderer.text(sep, x as f64, text_y as f64, &separator_style)?;
                x += sep_size.width as i32;
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
