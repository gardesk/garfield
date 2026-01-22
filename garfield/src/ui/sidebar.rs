//! Places sidebar with quick navigation to common directories.

use gartk_core::{Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::path::PathBuf;

/// A place in the sidebar.
#[derive(Debug, Clone)]
pub struct Place {
    /// Display name.
    pub name: String,
    /// Icon (emoji or text symbol).
    pub icon: String,
    /// Path to navigate to.
    pub path: PathBuf,
    /// Bounding rectangle for hit testing.
    bounds: Rect,
}

/// Places sidebar component.
pub struct Sidebar {
    /// List of places.
    places: Vec<Place>,
    /// Component bounds.
    bounds: Rect,
    /// Hovered item index.
    hovered: Option<usize>,
    /// Whether sidebar is visible.
    visible: bool,
    /// Item height.
    item_height: u32,
    /// Padding.
    padding: u32,
}

impl Sidebar {
    /// Create a new sidebar with default places.
    pub fn new(bounds: Rect) -> Self {
        let mut sidebar = Self {
            places: Vec::new(),
            bounds,
            hovered: None,
            visible: true,
            item_height: 32,
            padding: 8,
        };
        sidebar.populate_default_places();
        sidebar
    }

    /// Populate with default XDG directories.
    fn populate_default_places(&mut self) {
        self.places.clear();

        // Home directory
        if let Some(home) = dirs::home_dir() {
            self.places.push(Place {
                name: "Home".to_string(),
                icon: "~".to_string(),
                path: home,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Desktop
        if let Some(desktop) = dirs::desktop_dir() {
            self.places.push(Place {
                name: "Desktop".to_string(),
                icon: "D".to_string(),
                path: desktop,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Documents
        if let Some(docs) = dirs::document_dir() {
            self.places.push(Place {
                name: "Documents".to_string(),
                icon: "d".to_string(),
                path: docs,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Downloads
        if let Some(downloads) = dirs::download_dir() {
            self.places.push(Place {
                name: "Downloads".to_string(),
                icon: "v".to_string(),
                path: downloads,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Music
        if let Some(music) = dirs::audio_dir() {
            self.places.push(Place {
                name: "Music".to_string(),
                icon: "m".to_string(),
                path: music,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Pictures
        if let Some(pictures) = dirs::picture_dir() {
            self.places.push(Place {
                name: "Pictures".to_string(),
                icon: "p".to_string(),
                path: pictures,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Videos
        if let Some(videos) = dirs::video_dir() {
            self.places.push(Place {
                name: "Videos".to_string(),
                icon: "V".to_string(),
                path: videos,
                bounds: Rect::new(0, 0, 0, 0),
            });
        }

        // Root filesystem
        self.places.push(Place {
            name: "Filesystem".to_string(),
            icon: "/".to_string(),
            path: PathBuf::from("/"),
            bounds: Rect::new(0, 0, 0, 0),
        });

        // Trash (if available)
        if let Some(data_dir) = dirs::data_dir() {
            let trash_path = data_dir.join("Trash/files");
            if trash_path.exists() {
                self.places.push(Place {
                    name: "Trash".to_string(),
                    icon: "x".to_string(),
                    path: trash_path,
                    bounds: Rect::new(0, 0, 0, 0),
                });
            }
        }
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Get sidebar width.
    pub fn width(&self) -> u32 {
        if self.visible {
            self.bounds.width
        } else {
            0
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Handle mouse move for hover effects.
    pub fn on_mouse_move(&mut self, pos: Point) {
        if !self.visible || !self.bounds.contains_point(pos) {
            self.hovered = None;
            return;
        }

        self.hovered = self.places.iter().position(|p| p.bounds.contains_point(pos));
    }

    /// Handle mouse click. Returns the path to navigate to, if any.
    pub fn on_click(&self, pos: Point) -> Option<PathBuf> {
        if !self.visible || !self.bounds.contains_point(pos) {
            return None;
        }

        for place in &self.places {
            if place.bounds.contains_point(pos) {
                return Some(place.path.clone());
            }
        }

        None
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered = None;
    }

    /// Render the sidebar.
    pub fn render(&mut self, renderer: &Renderer) -> anyhow::Result<()> {
        if !self.visible {
            return Ok(());
        }

        let theme = renderer.theme();

        // Draw background
        renderer.fill_rect(self.bounds, theme.background)?;

        // Draw right border
        renderer.line(
            (self.bounds.x + self.bounds.width as i32) as f64,
            self.bounds.y as f64,
            (self.bounds.x + self.bounds.width as i32) as f64,
            (self.bounds.y + self.bounds.height as i32) as f64,
            theme.border,
            1.0,
        )?;

        // Text styles
        let icon_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground.with_alpha(0.7));

        let name_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.item_foreground);

        let hover_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.selection_background);

        // Render each place
        let mut y = self.bounds.y + self.padding as i32;

        for (i, place) in self.places.iter_mut().enumerate() {
            // Update bounds for hit testing
            place.bounds = Rect::new(
                self.bounds.x,
                y,
                self.bounds.width,
                self.item_height,
            );

            let is_hovered = self.hovered == Some(i);

            // Draw hover background
            if is_hovered {
                renderer.fill_rect(place.bounds, theme.item_background)?;
            }

            let text_style = if is_hovered { &hover_style } else { &name_style };

            // Draw icon
            let icon_x = self.bounds.x + self.padding as i32;
            let text_y = y + (self.item_height as i32 - theme.font_size as i32) / 2;
            renderer.text(&place.icon, icon_x as f64, text_y as f64, &icon_style)?;

            // Draw name
            let name_x = icon_x + 24;
            renderer.text(&place.name, name_x as f64, text_y as f64, text_style)?;

            y += self.item_height as i32;
        }

        Ok(())
    }
}
