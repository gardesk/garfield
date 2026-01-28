//! File drag controller for drag-to-move operations.
//!
//! Handles dragging files to directories or tabs with hover-to-enter behavior.

use gartk_core::{Point, Rect};
use std::path::PathBuf;
use std::time::Instant;

/// Drag activation threshold in pixels.
const DRAG_THRESHOLD: i32 = 5;

/// Flash animation duration in milliseconds (on or off).
const FLASH_DURATION_MS: u64 = 150;

/// Number of flash cycles before auto-enter.
const TOTAL_FLASHES: u8 = 3;

/// Target being hovered over during drag.
#[derive(Debug, Clone)]
pub enum DragTarget {
    /// A directory entry in the current view.
    Directory {
        path: PathBuf,
        bounds: Rect,
    },
    /// A tab in the tab bar.
    Tab {
        index: usize,
        target_path: PathBuf,
    },
    /// A breadcrumb segment in the path bar.
    Breadcrumb {
        path: PathBuf,
        bounds: Rect,
    },
}

impl DragTarget {
    /// Get the path associated with this target.
    pub fn path(&self) -> &PathBuf {
        match self {
            DragTarget::Directory { path, .. } => path,
            DragTarget::Tab { target_path, .. } => target_path,
            DragTarget::Breadcrumb { path, .. } => path,
        }
    }

    /// Check if this target matches a path.
    pub fn matches_path(&self, other: &PathBuf) -> bool {
        self.path() == other
    }
}

/// Flash animation state for hover feedback.
#[derive(Debug, Clone)]
pub struct FlashState {
    /// Number of complete flash cycles (0 to TOTAL_FLASHES).
    flash_count: u8,
    /// Time of last toggle.
    last_toggle: Instant,
    /// Whether highlight is currently visible.
    highlight_on: bool,
}

impl FlashState {
    /// Create a new flash state starting with highlight on.
    pub fn new() -> Self {
        Self {
            flash_count: 0,
            last_toggle: Instant::now(),
            highlight_on: true,
        }
    }

    /// Update flash state. Returns true if flash sequence is complete.
    pub fn update(&mut self) -> bool {
        if self.flash_count >= TOTAL_FLASHES {
            return true;
        }

        let elapsed = self.last_toggle.elapsed().as_millis() as u64;
        if elapsed >= FLASH_DURATION_MS {
            self.last_toggle = Instant::now();
            if self.highlight_on {
                // Turning off
                self.highlight_on = false;
            } else {
                // Turning on - counts as completing one cycle
                self.highlight_on = true;
                self.flash_count += 1;
            }
        }

        self.flash_count >= TOTAL_FLASHES
    }

    /// Check if highlight should be shown.
    pub fn is_highlighted(&self) -> bool {
        self.highlight_on
    }

    /// Get the flash count.
    pub fn count(&self) -> u8 {
        self.flash_count
    }
}

impl Default for FlashState {
    fn default() -> Self {
        Self::new()
    }
}

/// File drag state machine.
#[derive(Debug, Clone)]
pub enum FileDragState {
    /// No drag in progress.
    Idle,
    /// Mouse pressed on selected item, awaiting threshold.
    Pending {
        start_pos: Point,
        paths: Vec<PathBuf>,
    },
    /// Actively dragging files.
    Dragging {
        paths: Vec<PathBuf>,
        current_pos: Point,
    },
    /// Hovering over a drop target with flash animation.
    Hovering {
        paths: Vec<PathBuf>,
        current_pos: Point,
        target: DragTarget,
        flash_state: FlashState,
    },
}

/// Controller for file drag operations.
#[derive(Debug)]
pub struct FileDragController {
    state: FileDragState,
}

impl FileDragController {
    /// Create a new file drag controller.
    pub fn new() -> Self {
        Self {
            state: FileDragState::Idle,
        }
    }

    /// Get the current state.
    pub fn state(&self) -> &FileDragState {
        &self.state
    }

    /// Check if any drag operation is in progress (pending or active).
    pub fn is_active(&self) -> bool {
        !matches!(self.state, FileDragState::Idle)
    }

    /// Check if actively dragging (past threshold).
    pub fn is_dragging(&self) -> bool {
        matches!(self.state, FileDragState::Dragging { .. } | FileDragState::Hovering { .. })
    }

    /// Check if hovering over a target.
    pub fn is_hovering(&self) -> bool {
        matches!(self.state, FileDragState::Hovering { .. })
    }

    /// Get the dragged paths if dragging.
    pub fn dragged_paths(&self) -> Option<&Vec<PathBuf>> {
        match &self.state {
            FileDragState::Pending { paths, .. } => Some(paths),
            FileDragState::Dragging { paths, .. } => Some(paths),
            FileDragState::Hovering { paths, .. } => Some(paths),
            FileDragState::Idle => None,
        }
    }

    /// Get current mouse position during drag.
    pub fn current_pos(&self) -> Option<Point> {
        match &self.state {
            FileDragState::Dragging { current_pos, .. } => Some(*current_pos),
            FileDragState::Hovering { current_pos, .. } => Some(*current_pos),
            _ => None,
        }
    }

    /// Get the current hover target.
    pub fn current_target(&self) -> Option<&DragTarget> {
        match &self.state {
            FileDragState::Hovering { target, .. } => Some(target),
            _ => None,
        }
    }

    /// Check if a path is being targeted for highlight.
    pub fn is_target_path(&self, path: &PathBuf) -> bool {
        match &self.state {
            FileDragState::Hovering { target, .. } => target.matches_path(path),
            _ => false,
        }
    }

    /// Check if highlight should be shown for current target.
    pub fn should_show_highlight(&self) -> bool {
        match &self.state {
            FileDragState::Hovering { flash_state, .. } => flash_state.is_highlighted(),
            _ => false,
        }
    }

    /// Get the target tab index if hovering over a tab.
    pub fn target_tab_index(&self) -> Option<usize> {
        match &self.state {
            FileDragState::Hovering { target: DragTarget::Tab { index, .. }, .. } => Some(*index),
            _ => None,
        }
    }

    /// Start a potential file drag (on mouse press).
    pub fn start_pending(&mut self, pos: Point, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.state = FileDragState::Pending {
            start_pos: pos,
            paths,
        };
    }

    /// Update position during drag. Returns true if state changed.
    pub fn update_position(&mut self, pos: Point) -> bool {
        match &mut self.state {
            FileDragState::Pending { start_pos, paths } => {
                // Check if past drag threshold
                let dx = (pos.x - start_pos.x).abs();
                let dy = (pos.y - start_pos.y).abs();
                if dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD {
                    self.state = FileDragState::Dragging {
                        paths: paths.clone(),
                        current_pos: pos,
                    };
                    return true;
                }
                false
            }
            FileDragState::Dragging { current_pos, .. } => {
                *current_pos = pos;
                true
            }
            FileDragState::Hovering { current_pos, .. } => {
                *current_pos = pos;
                true
            }
            FileDragState::Idle => false,
        }
    }

    /// Set a hover target. Call when cursor is over a valid drop target.
    pub fn set_hover_target(&mut self, target: Option<DragTarget>) {
        match (&mut self.state, target) {
            (FileDragState::Dragging { paths, current_pos }, Some(new_target)) => {
                // Transition to hovering
                self.state = FileDragState::Hovering {
                    paths: paths.clone(),
                    current_pos: *current_pos,
                    target: new_target,
                    flash_state: FlashState::new(),
                };
            }
            (FileDragState::Hovering { paths, current_pos, target, .. }, Some(new_target)) => {
                // Check if target changed
                if !target.matches_path(new_target.path()) {
                    // New target - reset flash
                    self.state = FileDragState::Hovering {
                        paths: paths.clone(),
                        current_pos: *current_pos,
                        target: new_target,
                        flash_state: FlashState::new(),
                    };
                }
                // Same target - keep flash state
            }
            (FileDragState::Hovering { paths, current_pos, .. }, None) => {
                // Left target - go back to dragging
                self.state = FileDragState::Dragging {
                    paths: paths.clone(),
                    current_pos: *current_pos,
                };
            }
            _ => {}
        }
    }

    /// Update flash animation. Returns true if flash sequence completed.
    pub fn update_flash(&mut self) -> bool {
        if let FileDragState::Hovering { flash_state, .. } = &mut self.state {
            return flash_state.update();
        }
        false
    }

    /// Complete the drag operation. Returns the paths and target if valid drop.
    pub fn complete(&mut self) -> Option<(Vec<PathBuf>, DragTarget)> {
        let result = match &self.state {
            FileDragState::Hovering { paths, target, .. } => {
                Some((paths.clone(), target.clone()))
            }
            FileDragState::Dragging { paths, .. } => {
                // Dropped while dragging but not on a target
                // Could potentially use current position to find target
                None
            }
            _ => None,
        };
        self.cancel();
        result
    }

    /// Complete drag and get current target for auto-enter.
    /// Unlike complete(), this returns target info without consuming the drag state.
    pub fn get_auto_enter_target(&self) -> Option<DragTarget> {
        match &self.state {
            FileDragState::Hovering { target, .. } => Some(target.clone()),
            _ => None,
        }
    }

    /// Continue dragging after auto-enter (changes directory but keeps dragging).
    pub fn continue_after_enter(&mut self) {
        if let FileDragState::Hovering { paths, current_pos, .. } = &self.state {
            self.state = FileDragState::Dragging {
                paths: paths.clone(),
                current_pos: *current_pos,
            };
        }
    }

    /// Cancel the drag operation.
    pub fn cancel(&mut self) {
        self.state = FileDragState::Idle;
    }
}

impl Default for FileDragController {
    fn default() -> Self {
        Self::new()
    }
}
