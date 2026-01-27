//! Places sidebar with quick navigation to common directories and bookmarks.

use gartk_core::{Point, Rect};
use gartk_render::{Renderer, TextStyle};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// A place or bookmark in the sidebar.
#[derive(Debug, Clone)]
pub struct Place {
    /// Display name.
    pub name: String,
    /// Icon (text symbol).
    pub icon: String,
    /// Path to navigate to.
    pub path: PathBuf,
    /// Bounding rectangle for hit testing.
    bounds: Rect,
    /// Whether this is a bookmark (can be removed).
    pub is_bookmark: bool,
}

/// Places sidebar component.
pub struct Sidebar {
    /// Built-in places (XDG directories).
    places: Vec<Place>,
    /// User bookmarks.
    bookmarks: Vec<Place>,
    /// Component bounds.
    bounds: Rect,
    /// Hovered item index (in combined list).
    hovered: Option<usize>,
    /// Whether sidebar is visible.
    visible: bool,
    /// Item height.
    item_height: u32,
    /// Padding.
    padding: u32,
    /// Path to bookmarks file.
    bookmarks_path: PathBuf,
    /// Y coordinate where bookmarks section starts (for drop zone detection).
    bookmarks_section_y: i32,
    /// Whether to show drop highlight on bookmarks section.
    drop_highlight: bool,
    /// Index of bookmark being dragged for reorder.
    bookmark_drag_index: Option<usize>,
    /// Target insert position for bookmark reorder.
    bookmark_drop_index: Option<usize>,
    /// Starting position of bookmark drag.
    bookmark_drag_start: Option<Point>,
    /// Whether bookmark drag is active (past threshold).
    bookmark_drag_active: bool,
}

impl Sidebar {
    /// Create a new sidebar with default places.
    pub fn new(bounds: Rect) -> Self {
        let bookmarks_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("garfield")
            .join("bookmarks");

        let mut sidebar = Self {
            places: Vec::new(),
            bookmarks: Vec::new(),
            bounds,
            hovered: None,
            visible: true,
            item_height: 28,
            padding: 8,
            bookmarks_path,
            bookmarks_section_y: 0,
            drop_highlight: false,
            bookmark_drag_index: None,
            bookmark_drop_index: None,
            bookmark_drag_start: None,
            bookmark_drag_active: false,
        };
        sidebar.populate_default_places();
        sidebar.load_bookmarks();
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
                is_bookmark: false,
            });
        }

        // Desktop
        if let Some(desktop) = dirs::desktop_dir() {
            self.places.push(Place {
                name: "Desktop".to_string(),
                icon: "D".to_string(),
                path: desktop,
                bounds: Rect::new(0, 0, 0, 0),
                is_bookmark: false,
            });
        }

        // Documents
        if let Some(docs) = dirs::document_dir() {
            self.places.push(Place {
                name: "Documents".to_string(),
                icon: "d".to_string(),
                path: docs,
                bounds: Rect::new(0, 0, 0, 0),
                is_bookmark: false,
            });
        }

        // Downloads
        if let Some(downloads) = dirs::download_dir() {
            self.places.push(Place {
                name: "Downloads".to_string(),
                icon: "v".to_string(),
                path: downloads,
                bounds: Rect::new(0, 0, 0, 0),
                is_bookmark: false,
            });
        }

        // Music
        if let Some(music) = dirs::audio_dir() {
            self.places.push(Place {
                name: "Music".to_string(),
                icon: "m".to_string(),
                path: music,
                bounds: Rect::new(0, 0, 0, 0),
                is_bookmark: false,
            });
        }

        // Pictures
        if let Some(pictures) = dirs::picture_dir() {
            self.places.push(Place {
                name: "Pictures".to_string(),
                icon: "p".to_string(),
                path: pictures,
                bounds: Rect::new(0, 0, 0, 0),
                is_bookmark: false,
            });
        }

        // Videos
        if let Some(videos) = dirs::video_dir() {
            self.places.push(Place {
                name: "Videos".to_string(),
                icon: "V".to_string(),
                path: videos,
                bounds: Rect::new(0, 0, 0, 0),
                is_bookmark: false,
            });
        }

        // Root filesystem
        self.places.push(Place {
            name: "Filesystem".to_string(),
            icon: "/".to_string(),
            path: PathBuf::from("/"),
            bounds: Rect::new(0, 0, 0, 0),
            is_bookmark: false,
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
                    is_bookmark: false,
                });
            }
        }

        // Mounted volumes
        self.add_mounted_volumes();
    }

    /// Add mounted volumes from common mount points.
    fn add_mounted_volumes(&mut self) {
        let mount_points = [
            PathBuf::from("/media"),
            PathBuf::from("/mnt"),
        ];

        // Also check /run/media/$USER for modern systems
        if let Ok(username) = std::env::var("USER") {
            let user_media = PathBuf::from(format!("/run/media/{}", username));
            if user_media.exists() {
                self.scan_mount_point(&user_media);
            }
        }

        for mount_point in &mount_points {
            if mount_point.exists() {
                self.scan_mount_point(mount_point);
            }
        }
    }

    /// Scan a mount point directory for mounted volumes.
    fn scan_mount_point(&mut self, mount_point: &Path) {
        if let Ok(entries) = fs::read_dir(mount_point) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Volume".to_string());

                    // Skip if already in places (avoid duplicates)
                    if self.places.iter().any(|p| p.path == path) {
                        continue;
                    }

                    self.places.push(Place {
                        name,
                        icon: "#".to_string(), // Volume/drive icon
                        path,
                        bounds: Rect::new(0, 0, 0, 0),
                        is_bookmark: false,
                    });
                }
            }
        }
    }

    /// Load bookmarks from config file.
    fn load_bookmarks(&mut self) {
        self.bookmarks.clear();

        if let Ok(file) = fs::File::open(&self.bookmarks_path) {
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let path = PathBuf::from(line);
                if path.exists() {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| line.to_string());

                    self.bookmarks.push(Place {
                        name,
                        icon: "*".to_string(),
                        path,
                        bounds: Rect::new(0, 0, 0, 0),
                        is_bookmark: true,
                    });
                }
            }
        }
    }

    /// Save bookmarks to config file.
    fn save_bookmarks(&self) {
        // Ensure config directory exists
        if let Some(parent) = self.bookmarks_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(mut file) = fs::File::create(&self.bookmarks_path) {
            for bookmark in &self.bookmarks {
                let _ = writeln!(file, "{}", bookmark.path.display());
            }
        }
    }

    /// Add a bookmark for the given path. Returns true if added.
    pub fn add_bookmark(&mut self, path: &Path) -> bool {
        // Canonicalize the path for consistent comparison
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Check if already bookmarked
        if self.bookmarks.iter().any(|b| {
            b.path.canonicalize().unwrap_or_else(|_| b.path.clone()) == canonical
        }) {
            return false;
        }

        // Check if it's a default place
        if self.places.iter().any(|p| {
            p.path.canonicalize().unwrap_or_else(|_| p.path.clone()) == canonical
        }) {
            return false;
        }

        let name = canonical
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| canonical.to_string_lossy().to_string());

        self.bookmarks.push(Place {
            name,
            icon: "*".to_string(),
            path: canonical,
            bounds: Rect::new(0, 0, 0, 0),
            is_bookmark: true,
        });

        self.save_bookmarks();
        true
    }

    /// Remove a bookmark by path. Returns true if removed.
    pub fn remove_bookmark(&mut self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let initial_len = self.bookmarks.len();
        self.bookmarks.retain(|b| {
            b.path.canonicalize().unwrap_or_else(|_| b.path.clone()) != canonical
        });

        if self.bookmarks.len() != initial_len {
            self.save_bookmarks();
            true
        } else {
            false
        }
    }

    /// Check if a path is bookmarked.
    pub fn is_bookmarked(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.bookmarks.iter().any(|b| {
            b.path.canonicalize().unwrap_or_else(|_| b.path.clone()) == canonical
        })
    }

    /// Toggle bookmark for a path.
    pub fn toggle_bookmark(&mut self, path: &Path) -> bool {
        if self.is_bookmarked(path) {
            self.remove_bookmark(path);
            false
        } else {
            self.add_bookmark(path);
            true
        }
    }

    /// Get the bookmark index at a given point (if any).
    fn bookmark_index_at_point(&self, pos: Point) -> Option<usize> {
        for (i, bookmark) in self.bookmarks.iter().enumerate() {
            if bookmark.bounds.contains_point(pos) {
                return Some(i);
            }
        }
        None
    }

    /// Start dragging a bookmark if the position is over one.
    /// Returns true if drag was started.
    pub fn start_bookmark_drag(&mut self, pos: Point) -> bool {
        if !self.visible || !self.bounds.contains_point(pos) {
            return false;
        }

        if let Some(index) = self.bookmark_index_at_point(pos) {
            self.bookmark_drag_index = Some(index);
            self.bookmark_drag_start = Some(pos);
            self.bookmark_drag_active = false;
            self.bookmark_drop_index = None;
            true
        } else {
            false
        }
    }

    /// Update bookmark drag state with current mouse position.
    /// Returns true if drag is active.
    pub fn update_bookmark_drag(&mut self, pos: Point) -> bool {
        if self.bookmark_drag_index.is_none() {
            return false;
        }

        // Check if past drag threshold
        if !self.bookmark_drag_active {
            if let Some(start) = self.bookmark_drag_start {
                let dx = (pos.x - start.x).abs();
                let dy = (pos.y - start.y).abs();
                if dx > 5 || dy > 5 {
                    self.bookmark_drag_active = true;
                }
            }
        }

        if !self.bookmark_drag_active {
            return false;
        }

        // Calculate drop index based on mouse position
        if pos.y < self.bookmarks_section_y || !self.bounds.contains_point(pos) {
            self.bookmark_drop_index = None;
        } else {
            // Find which slot we're closest to
            let mut drop_index = 0;
            for (i, bookmark) in self.bookmarks.iter().enumerate() {
                let mid_y = bookmark.bounds.y + bookmark.bounds.height as i32 / 2;
                if pos.y > mid_y {
                    drop_index = i + 1;
                }
            }
            self.bookmark_drop_index = Some(drop_index);
        }

        true
    }

    /// Complete bookmark drag and reorder.
    /// Returns true if reorder occurred.
    pub fn complete_bookmark_drag(&mut self) -> bool {
        let result = if self.bookmark_drag_active {
            if let (Some(from), Some(to)) = (self.bookmark_drag_index, self.bookmark_drop_index) {
                if from != to && to != from + 1 && !self.bookmarks.is_empty() {
                    // Perform the reorder
                    let bookmark = self.bookmarks.remove(from);
                    let new_index = if to > from { to - 1 } else { to };
                    self.bookmarks.insert(new_index, bookmark);
                    self.save_bookmarks();
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

        self.cancel_bookmark_drag();
        result
    }

    /// Cancel bookmark drag.
    pub fn cancel_bookmark_drag(&mut self) {
        self.bookmark_drag_index = None;
        self.bookmark_drop_index = None;
        self.bookmark_drag_start = None;
        self.bookmark_drag_active = false;
    }

    /// Check if bookmark drag is in progress.
    pub fn is_bookmark_dragging(&self) -> bool {
        self.bookmark_drag_active
    }

    /// Get the index of the bookmark being dragged (for checking if a click was on a bookmark).
    pub fn bookmark_drag_index(&self) -> Option<usize> {
        self.bookmark_drag_index
    }

    /// Get the path of the bookmark at the current drag index.
    pub fn bookmark_path_at_index(&self) -> Option<PathBuf> {
        self.bookmark_drag_index
            .and_then(|i| self.bookmarks.get(i))
            .map(|b| b.path.clone())
    }

    /// Update bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Get current bounds.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Get sidebar width.
    pub fn width(&self) -> u32 {
        if self.visible {
            self.bounds.width
        } else {
            0
        }
    }

    /// Set sidebar width (clamped to min/max).
    pub fn set_width(&mut self, width: u32) {
        let min_width = 120;
        let max_width = 400;
        self.bounds.width = width.clamp(min_width, max_width);
    }

    /// Check if position is on the resize handle (right edge).
    pub fn is_resize_handle(&self, pos: Point) -> bool {
        if !self.visible {
            return false;
        }
        let handle_x = self.bounds.x + self.bounds.width as i32;
        let tolerance = 4;
        pos.x >= handle_x - tolerance
            && pos.x <= handle_x + tolerance
            && pos.y >= self.bounds.y
            && pos.y <= self.bounds.y + self.bounds.height as i32
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Total number of items (places + bookmarks + separator if bookmarks exist).
    fn total_items(&self) -> usize {
        self.places.len() + self.bookmarks.len()
    }

    /// Get item by combined index.
    fn get_item(&self, index: usize) -> Option<&Place> {
        if index < self.places.len() {
            self.places.get(index)
        } else {
            self.bookmarks.get(index - self.places.len())
        }
    }

    /// Get mutable item by combined index.
    fn get_item_mut(&mut self, index: usize) -> Option<&mut Place> {
        let places_len = self.places.len();
        if index < places_len {
            self.places.get_mut(index)
        } else {
            self.bookmarks.get_mut(index - places_len)
        }
    }

    /// Handle mouse move for hover effects. Returns true if hover state changed.
    pub fn on_mouse_move(&mut self, pos: Point) -> bool {
        let old_hovered = self.hovered;

        if !self.visible || !self.bounds.contains_point(pos) {
            self.hovered = None;
            return self.hovered != old_hovered;
        }

        self.hovered = None;
        for i in 0..self.total_items() {
            if let Some(place) = self.get_item(i) {
                if place.bounds.contains_point(pos) {
                    self.hovered = Some(i);
                    break;
                }
            }
        }

        self.hovered != old_hovered
    }

    /// Handle mouse scroll. Returns true if scrolled.
    /// Sidebar doesn't currently support scrolling, but this handles the event.
    pub fn on_scroll(&mut self, _delta_y: i32) -> bool {
        false // Sidebar doesn't scroll currently
    }

    /// Handle mouse click. Returns the path to navigate to, if any.
    pub fn on_click(&self, pos: Point) -> Option<PathBuf> {
        if !self.visible || !self.bounds.contains_point(pos) {
            return None;
        }

        for i in 0..self.total_items() {
            if let Some(place) = self.get_item(i) {
                if place.bounds.contains_point(pos) {
                    return Some(place.path.clone());
                }
            }
        }

        None
    }

    /// Clear hover state.
    pub fn clear_hover(&mut self) {
        self.hovered = None;
    }

    /// Check if the given position is within the bookmarks drop zone.
    pub fn is_bookmark_drop_zone(&self, pos: Point) -> bool {
        if !self.visible {
            return false;
        }

        // Check if within sidebar bounds
        if !self.bounds.contains_point(pos) {
            return false;
        }

        // Check if below the bookmarks section start
        pos.y >= self.bookmarks_section_y
    }

    /// Set whether to show the drop highlight on the bookmarks section.
    pub fn set_drop_highlight(&mut self, highlight: bool) {
        self.drop_highlight = highlight;
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

        let header_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 1.0)
            .color(theme.item_foreground.with_alpha(0.5));

        let mut y = self.bounds.y + self.padding as i32;

        // Render places
        for i in 0..self.places.len() {
            let is_hovered = self.hovered == Some(i);
            y = self.render_item(renderer, i, y, is_hovered, &icon_style, &name_style, &hover_style)?;
        }

        // Always show separator and bookmarks section
        y += 8;
        renderer.line(
            (self.bounds.x + self.padding as i32) as f64,
            y as f64,
            (self.bounds.x + self.bounds.width as i32 - self.padding as i32) as f64,
            y as f64,
            theme.border,
            1.0,
        )?;
        y += 8;

        // Store the bookmarks section start for drop zone detection
        self.bookmarks_section_y = y;

        // Draw drop highlight if active
        if self.drop_highlight {
            let highlight_rect = Rect::new(
                self.bounds.x,
                y,
                self.bounds.width,
                (self.bounds.y + self.bounds.height as i32 - y) as u32,
            );
            renderer.fill_rect(highlight_rect, theme.selection_background.with_alpha(0.2))?;
        }

        // Header
        let header_x = self.bounds.x + self.padding as i32;
        renderer.text("Bookmarks", header_x as f64, y as f64, &header_style)?;
        y += (theme.font_size + 4.0) as i32;

        // Bookmark items (or hint if empty)
        if self.bookmarks.is_empty() {
            let hint_style = TextStyle::new()
                .font_family(&theme.font_family)
                .font_size(theme.font_size - 2.0)
                .color(theme.item_foreground.with_alpha(0.4));
            renderer.text("Ctrl+D to add", (header_x + 4) as f64, y as f64, &hint_style)?;

            // Draw drop indicator at start if dragging (shouldn't happen but be safe)
            if self.bookmark_drag_active && self.bookmark_drop_index == Some(0) {
                self.render_drop_indicator(renderer, y)?;
            }
        } else {
            for i in 0..self.bookmarks.len() {
                // Draw drop indicator before this item if needed
                if self.bookmark_drag_active && self.bookmark_drop_index == Some(i) {
                    self.render_drop_indicator(renderer, y)?;
                }

                let combined_index = self.places.len() + i;
                let is_hovered = self.hovered == Some(combined_index);
                let is_dragging = self.bookmark_drag_index == Some(i);
                y = self.render_bookmark_item(renderer, combined_index, y, is_hovered, is_dragging, &icon_style, &name_style, &hover_style)?;
            }

            // Draw drop indicator at end if needed
            if self.bookmark_drag_active && self.bookmark_drop_index == Some(self.bookmarks.len()) {
                self.render_drop_indicator(renderer, y)?;
            }
        }

        Ok(())
    }

    /// Render a single sidebar item.
    fn render_item(
        &mut self,
        renderer: &Renderer,
        index: usize,
        y: i32,
        is_hovered: bool,
        icon_style: &TextStyle,
        name_style: &TextStyle,
        hover_style: &TextStyle,
    ) -> anyhow::Result<i32> {
        let theme = renderer.theme();

        // Get item (need to reborrow to avoid issues)
        let (icon, name) = {
            let item = self.get_item(index).unwrap();
            (item.icon.clone(), item.name.clone())
        };

        // Update bounds for hit testing
        let item_bounds = Rect::new(
            self.bounds.x,
            y,
            self.bounds.width,
            self.item_height,
        );

        // Store bounds
        if let Some(item) = self.get_item_mut(index) {
            item.bounds = item_bounds;
        }

        // Draw hover background
        if is_hovered {
            renderer.fill_rect(item_bounds, theme.item_background)?;
        }

        let text_style = if is_hovered { hover_style } else { name_style };

        // Draw icon
        let icon_x = self.bounds.x + self.padding as i32;
        let text_y = y + (self.item_height as i32 - theme.font_size as i32) / 2;
        renderer.text(&icon, icon_x as f64, text_y as f64, icon_style)?;

        // Draw name
        let name_x = icon_x + 20;
        renderer.text(&name, name_x as f64, text_y as f64, text_style)?;

        Ok(y + self.item_height as i32)
    }

    /// Render a bookmark item (supports dimming when being dragged).
    fn render_bookmark_item(
        &mut self,
        renderer: &Renderer,
        index: usize,
        y: i32,
        is_hovered: bool,
        is_dragging: bool,
        icon_style: &TextStyle,
        name_style: &TextStyle,
        hover_style: &TextStyle,
    ) -> anyhow::Result<i32> {
        let theme = renderer.theme();

        // Get item (need to reborrow to avoid issues)
        let (icon, name) = {
            let item = self.get_item(index).unwrap();
            (item.icon.clone(), item.name.clone())
        };

        // Update bounds for hit testing
        let item_bounds = Rect::new(
            self.bounds.x,
            y,
            self.bounds.width,
            self.item_height,
        );

        // Store bounds
        if let Some(item) = self.get_item_mut(index) {
            item.bounds = item_bounds;
        }

        // Draw hover background (unless being dragged)
        if is_hovered && !is_dragging {
            renderer.fill_rect(item_bounds, theme.item_background)?;
        }

        // Dim if being dragged
        let alpha = if is_dragging { 0.4 } else { 1.0 };

        let text_style = if is_hovered && !is_dragging {
            hover_style.clone()
        } else {
            name_style.clone().color(theme.item_foreground.with_alpha(alpha))
        };

        let icon_style = icon_style.clone().color(theme.item_foreground.with_alpha(0.7 * alpha));

        // Draw icon
        let icon_x = self.bounds.x + self.padding as i32;
        let text_y = y + (self.item_height as i32 - theme.font_size as i32) / 2;
        renderer.text(&icon, icon_x as f64, text_y as f64, &icon_style)?;

        // Draw name
        let name_x = icon_x + 20;
        renderer.text(&name, name_x as f64, text_y as f64, &text_style)?;

        Ok(y + self.item_height as i32)
    }

    /// Render a drop indicator line.
    fn render_drop_indicator(&self, renderer: &Renderer, y: i32) -> anyhow::Result<()> {
        let theme = renderer.theme();
        let line_y = y - 2;
        renderer.line(
            (self.bounds.x + self.padding as i32) as f64,
            line_y as f64,
            (self.bounds.x + self.bounds.width as i32 - self.padding as i32) as f64,
            line_y as f64,
            theme.selection_background,
            2.0,
        )?;
        Ok(())
    }
}
