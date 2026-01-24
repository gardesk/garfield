//! Application picker dialog for "Open With" functionality.
//!
//! Provides a mini garlaunch-style dialog that scans for installed applications
//! and allows the user to search and select one to open a file with.

use anyhow::Result;
use freedesktop_entry_parser::Entry;
use gartk_core::{Key, Point, Rect};
use gartk_render::{Renderer, TextStyle};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use nucleo_matcher::pattern::{Pattern, CaseMatching, Normalization};
use std::collections::HashSet;
use std::path::PathBuf;

/// Maximum number of visible items in the list.
const MAX_VISIBLE_ITEMS: usize = 10;

/// Item height in pixels.
const ITEM_HEIGHT: u32 = 36;

/// Input field height.
const INPUT_HEIGHT: u32 = 40;

/// Padding inside dialog.
const DIALOG_PADDING: u32 = 16;

/// An installed application entry.
#[derive(Debug, Clone)]
pub struct AppEntry {
    /// Display name from .desktop file.
    pub name: String,
    /// Optional description/comment.
    pub description: Option<String>,
    /// Exec command (cleaned of field codes).
    pub exec: String,
    /// Icon name (not currently rendered).
    pub icon: Option<String>,
    /// Path to the .desktop file.
    pub desktop_path: PathBuf,
}

impl AppEntry {
    /// Parse an AppEntry from a .desktop file.
    fn from_desktop_file(path: &PathBuf) -> Option<Self> {
        let entry = Entry::parse_file(path).ok()?;
        let section = entry.section("Desktop Entry");

        // Skip hidden or no-display entries
        if section.attr("NoDisplay") == Some("true") {
            return None;
        }
        if section.attr("Hidden") == Some("true") {
            return None;
        }

        // Must have Name and Exec
        let name = section.attr("Name")?.to_string();
        let exec_raw = section.attr("Exec")?;

        // Clean exec command - remove field codes like %f, %F, %u, %U, etc.
        let exec = clean_exec_command(exec_raw);

        let description = section.attr("Comment").map(String::from);
        let icon = section.attr("Icon").map(String::from);

        Some(AppEntry {
            name,
            description,
            exec,
            icon,
            desktop_path: path.clone(),
        })
    }
}

/// Clean field codes from an Exec command.
fn clean_exec_command(exec: &str) -> String {
    let mut result = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Skip the field code character
            if let Some(&next) = chars.peek() {
                match next {
                    'f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'i' | 'c' | 'k' | 'v' | 'm' => {
                        chars.next();
                        continue;
                    }
                    '%' => {
                        // %% becomes %
                        chars.next();
                        result.push('%');
                        continue;
                    }
                    _ => {}
                }
            }
        }
        result.push(c);
    }

    result.trim().to_string()
}

/// Get XDG application directories to scan.
fn get_app_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // User applications
    if let Some(data_home) = dirs::data_dir() {
        dirs.push(data_home.join("applications"));
    }

    // System applications
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/local/share/applications"));

    // NixOS
    dirs.push(PathBuf::from("/run/current-system/sw/share/applications"));

    // Flatpak user
    if let Some(data_home) = dirs::data_dir() {
        dirs.push(data_home.join("flatpak/exports/share/applications"));
    }

    // Flatpak system
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    // Snap
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    dirs
}

/// Scan for installed applications.
fn scan_applications() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let mut seen_files: HashSet<String> = HashSet::new();

    for dir in get_app_directories() {
        if !dir.exists() {
            continue;
        }

        // Use walkdir to handle nested directories
        for entry in walkdir::WalkDir::new(&dir)
            .follow_links(true)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only process .desktop files
            if path.extension().map(|e| e != "desktop").unwrap_or(true) {
                continue;
            }

            // Deduplicate by filename
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if seen_files.contains(&filename) {
                continue;
            }
            seen_files.insert(filename);

            // Parse the entry
            let path_buf = path.to_path_buf();
            if let Some(app) = AppEntry::from_desktop_file(&path_buf) {
                apps.push(app);
            }
        }
    }

    // Sort by name
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// Result of app picker interaction.
#[derive(Debug, Clone)]
pub enum AppPickerResult {
    /// User selected an application.
    Selected(String),
    /// User cancelled.
    Cancelled,
}

/// A modal dialog for picking an application.
pub struct AppPickerDialog {
    /// Window bounds (for centering).
    bounds: Rect,
    /// All discovered applications.
    all_apps: Vec<AppEntry>,
    /// Filtered applications (matching search).
    filtered_apps: Vec<usize>,
    /// Search input text.
    input: String,
    /// Cursor position in input.
    cursor: usize,
    /// Selected item index in filtered list.
    selected: usize,
    /// Scroll offset for list.
    scroll_offset: usize,
    /// Whether the dialog is visible.
    visible: bool,
    /// Fuzzy matcher.
    matcher: Matcher,
    /// Hovered item index.
    hovered_index: Option<usize>,
}

impl AppPickerDialog {
    /// Create a new app picker dialog.
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            all_apps: Vec::new(),
            filtered_apps: Vec::new(),
            input: String::new(),
            cursor: 0,
            selected: 0,
            scroll_offset: 0,
            visible: false,
            matcher: Matcher::new(Config::DEFAULT),
            hovered_index: None,
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Check if visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Show the dialog.
    pub fn show(&mut self) {
        // Scan for applications if not already loaded
        if self.all_apps.is_empty() {
            self.all_apps = scan_applications();
        }

        // Reset state
        self.input.clear();
        self.cursor = 0;
        self.selected = 0;
        self.scroll_offset = 0;
        self.hovered_index = None;

        // Initially show all apps
        self.filtered_apps = (0..self.all_apps.len()).collect();

        self.visible = true;
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Reload applications list.
    pub fn reload(&mut self) {
        self.all_apps = scan_applications();
        self.filter_apps();
    }

    /// Filter applications based on current input.
    fn filter_apps(&mut self) {
        if self.input.is_empty() {
            // Show all apps when no input
            self.filtered_apps = (0..self.all_apps.len()).collect();
        } else {
            // Use nucleo for fuzzy matching
            let pattern = Pattern::new(
                &self.input,
                CaseMatching::Smart,
                Normalization::Smart,
                nucleo_matcher::pattern::AtomKind::Fuzzy,
            );

            let mut matches: Vec<(usize, u32)> = self.all_apps
                .iter()
                .enumerate()
                .filter_map(|(idx, app)| {
                    let mut buf = Vec::new();
                    let haystack = Utf32Str::new(&app.name, &mut buf);
                    let score = pattern.score(haystack, &mut self.matcher)?;

                    // Also try matching description
                    let desc_score = app.description.as_ref().and_then(|desc| {
                        let mut desc_buf = Vec::new();
                        let desc_haystack = Utf32Str::new(desc, &mut desc_buf);
                        pattern.score(desc_haystack, &mut self.matcher)
                    }).unwrap_or(0);

                    Some((idx, score.max(desc_score)))
                })
                .collect();

            // Sort by score descending
            matches.sort_by(|a, b| b.1.cmp(&a.1));

            self.filtered_apps = matches.into_iter().map(|(idx, _)| idx).collect();
        }

        // Reset selection
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Handle key press. Returns Some(result) if dialog should close.
    pub fn handle_key(&mut self, key: &Key) -> Option<AppPickerResult> {
        if !self.visible {
            return None;
        }

        match key {
            Key::Escape => {
                self.hide();
                Some(AppPickerResult::Cancelled)
            }
            Key::Return => {
                if let Some(&idx) = self.filtered_apps.get(self.selected) {
                    if let Some(app) = self.all_apps.get(idx) {
                        let exec = app.exec.clone();
                        self.hide();
                        return Some(AppPickerResult::Selected(exec));
                    }
                }
                None
            }
            Key::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.ensure_visible();
                }
                None
            }
            Key::Down => {
                if self.selected + 1 < self.filtered_apps.len() {
                    self.selected += 1;
                    self.ensure_visible();
                }
                None
            }
            Key::PageUp => {
                self.selected = self.selected.saturating_sub(MAX_VISIBLE_ITEMS);
                self.ensure_visible();
                None
            }
            Key::PageDown => {
                self.selected = (self.selected + MAX_VISIBLE_ITEMS)
                    .min(self.filtered_apps.len().saturating_sub(1));
                self.ensure_visible();
                None
            }
            Key::Char(c) => {
                self.input.insert(self.cursor, *c);
                self.cursor += 1;
                self.filter_apps();
                None
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.input.remove(self.cursor);
                    self.filter_apps();
                }
                None
            }
            Key::Delete => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                    self.filter_apps();
                }
                None
            }
            Key::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                None
            }
            Key::Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
                None
            }
            Key::Home => {
                self.cursor = 0;
                None
            }
            Key::End => {
                self.cursor = self.input.len();
                None
            }
            _ => None,
        }
    }

    /// Ensure the selected item is visible.
    fn ensure_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + MAX_VISIBLE_ITEMS {
            self.scroll_offset = self.selected - MAX_VISIBLE_ITEMS + 1;
        }
    }

    /// Handle mouse click. Returns Some(result) if dialog should close.
    pub fn on_click(&mut self, pos: Point) -> Option<AppPickerResult> {
        if !self.visible {
            return None;
        }

        let dialog_rect = self.dialog_rect();

        // Click outside closes dialog
        if !dialog_rect.contains_point(pos) {
            self.hide();
            return Some(AppPickerResult::Cancelled);
        }

        // Check if click is in item list area
        let list_y_start = dialog_rect.y + DIALOG_PADDING as i32 + INPUT_HEIGHT as i32 + 8;
        let list_y_end = list_y_start + (MAX_VISIBLE_ITEMS as i32 * ITEM_HEIGHT as i32);

        if pos.y >= list_y_start && pos.y < list_y_end {
            let relative_y = pos.y - list_y_start;
            let clicked_index = (relative_y / ITEM_HEIGHT as i32) as usize + self.scroll_offset;

            if clicked_index < self.filtered_apps.len() {
                if let Some(&idx) = self.filtered_apps.get(clicked_index) {
                    if let Some(app) = self.all_apps.get(idx) {
                        let exec = app.exec.clone();
                        self.hide();
                        return Some(AppPickerResult::Selected(exec));
                    }
                }
            }
        }

        // Check if click is in input area
        let input_rect = self.input_rect();
        if input_rect.contains_point(pos) {
            // Could implement click-to-position cursor here
            return None;
        }

        None
    }

    /// Handle mouse move.
    pub fn on_mouse_move(&mut self, pos: Point) {
        if !self.visible {
            return;
        }

        let dialog_rect = self.dialog_rect();
        let list_y_start = dialog_rect.y + DIALOG_PADDING as i32 + INPUT_HEIGHT as i32 + 8;
        let list_y_end = list_y_start + (MAX_VISIBLE_ITEMS as i32 * ITEM_HEIGHT as i32);

        if pos.y >= list_y_start && pos.y < list_y_end
            && pos.x >= dialog_rect.x + DIALOG_PADDING as i32
            && pos.x < dialog_rect.x + dialog_rect.width as i32 - DIALOG_PADDING as i32
        {
            let relative_y = pos.y - list_y_start;
            let hovered = (relative_y / ITEM_HEIGHT as i32) as usize + self.scroll_offset;

            if hovered < self.filtered_apps.len() {
                self.hovered_index = Some(hovered);
            } else {
                self.hovered_index = None;
            }
        } else {
            self.hovered_index = None;
        }
    }

    /// Get the dialog rectangle (centered).
    fn dialog_rect(&self) -> Rect {
        let dialog_width = 500.min(self.bounds.width.saturating_sub(40));
        let dialog_height = (DIALOG_PADDING * 2 + INPUT_HEIGHT + 8 + (MAX_VISIBLE_ITEMS as u32 * ITEM_HEIGHT) + 24)
            .min(self.bounds.height.saturating_sub(40));

        let x = self.bounds.x + (self.bounds.width as i32 - dialog_width as i32) / 2;
        let y = self.bounds.y + (self.bounds.height as i32 - dialog_height as i32) / 3; // Upper third

        Rect::new(x, y, dialog_width, dialog_height)
    }

    /// Get the input field rectangle.
    fn input_rect(&self) -> Rect {
        let dialog = self.dialog_rect();
        Rect::new(
            dialog.x + DIALOG_PADDING as i32,
            dialog.y + DIALOG_PADDING as i32,
            dialog.width - DIALOG_PADDING * 2,
            INPUT_HEIGHT,
        )
    }

    /// Render the dialog.
    pub fn render(&self, renderer: &Renderer) -> Result<()> {
        if !self.visible {
            return Ok(());
        }

        let theme = renderer.theme();
        let dialog_rect = self.dialog_rect();

        // Dim background overlay
        renderer.fill_rect(self.bounds, gartk_core::Color::from_u8(0, 0, 0, 180))?;

        // Dialog background
        renderer.fill_rounded_rect(dialog_rect, 8.0, theme.background)?;
        renderer.stroke_rounded_rect(dialog_rect, 8.0, theme.border, 1.0)?;

        // Title
        let title_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size + 2.0)
            .color(theme.foreground);

        renderer.text(
            "Open With Application",
            (dialog_rect.x + DIALOG_PADDING as i32) as f64,
            (dialog_rect.y + 8) as f64,
            &title_style,
        )?;

        // Input field
        let input_rect = self.input_rect();
        renderer.fill_rounded_rect(input_rect, 4.0, theme.item_background)?;
        renderer.stroke_rounded_rect(input_rect, 4.0, theme.selection_background, 2.0)?;

        let input_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        // Prompt and input text
        let prompt = "Search: ";
        renderer.text(
            prompt,
            (input_rect.x + 12) as f64,
            (input_rect.y + 10) as f64,
            &input_style,
        )?;

        let prompt_width = renderer.measure_text(prompt, &input_style)
            .map(|m| m.width as i32)
            .unwrap_or(60);

        let input_text_x = input_rect.x + 12 + prompt_width;
        renderer.text(
            &self.input,
            input_text_x as f64,
            (input_rect.y + 10) as f64,
            &input_style,
        )?;

        // Cursor
        let cursor_text = &self.input[..self.cursor];
        let cursor_offset = if cursor_text.is_empty() {
            0
        } else {
            renderer.measure_text(cursor_text, &input_style)
                .map(|m| m.width as i32)
                .unwrap_or(0)
        };

        let cursor_x = input_text_x + cursor_offset;
        renderer.line(
            cursor_x as f64,
            (input_rect.y + 8) as f64,
            cursor_x as f64,
            (input_rect.y + INPUT_HEIGHT as i32 - 8) as f64,
            theme.foreground,
            1.0,
        )?;

        // Item list
        let list_y_start = dialog_rect.y + DIALOG_PADDING as i32 + INPUT_HEIGHT as i32 + 8;
        let list_width = dialog_rect.width - DIALOG_PADDING * 2;

        let name_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size)
            .color(theme.foreground);

        let desc_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 2.0)
            .color(theme.item_description);

        let visible_end = (self.scroll_offset + MAX_VISIBLE_ITEMS).min(self.filtered_apps.len());

        for (i, &app_idx) in self.filtered_apps[self.scroll_offset..visible_end].iter().enumerate() {
            let app = &self.all_apps[app_idx];
            let item_y = list_y_start + (i as i32 * ITEM_HEIGHT as i32);
            let display_idx = self.scroll_offset + i;

            let item_rect = Rect::new(
                dialog_rect.x + DIALOG_PADDING as i32,
                item_y,
                list_width,
                ITEM_HEIGHT,
            );

            // Highlight selected or hovered item
            let is_selected = display_idx == self.selected;
            let is_hovered = self.hovered_index == Some(display_idx);

            if is_selected {
                renderer.fill_rounded_rect(item_rect, 4.0, theme.selection_background)?;
            } else if is_hovered {
                renderer.fill_rounded_rect(item_rect, 4.0, theme.item_hover_background)?;
            }

            // App name
            renderer.text(
                &app.name,
                (item_rect.x + 12) as f64,
                (item_y + 6) as f64,
                &name_style,
            )?;

            // App description (if any)
            if let Some(desc) = &app.description {
                // Truncate long descriptions
                let max_desc_len = 60;
                let truncated = if desc.len() > max_desc_len {
                    format!("{}...", &desc[..max_desc_len])
                } else {
                    desc.clone()
                };

                renderer.text(
                    &truncated,
                    (item_rect.x + 12) as f64,
                    (item_y + 6 + theme.font_size as i32) as f64,
                    &desc_style,
                )?;
            }
        }

        // Item count
        let count_text = format!("{} / {} applications", self.filtered_apps.len(), self.all_apps.len());
        let count_style = TextStyle::new()
            .font_family(&theme.font_family)
            .font_size(theme.font_size - 2.0)
            .color(theme.item_description);

        let count_y = dialog_rect.y + dialog_rect.height as i32 - 20;
        renderer.text(
            &count_text,
            (dialog_rect.x + DIALOG_PADDING as i32) as f64,
            count_y as f64,
            &count_style,
        )?;

        Ok(())
    }
}
