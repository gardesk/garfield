//! Pane management with split support.

use crate::ui::tab::Tab;
use gartk_core::{Point, Rect};
use gartk_render::Renderer;
use std::path::PathBuf;

/// Split direction for panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Minimum pane size (width or height).
pub const MIN_PANE_SIZE: u32 = 100;

/// Divider size for split panes.
const DIVIDER_SIZE: u32 = 4;

/// A pane that can be a leaf (with tabs) or a split (with two child panes).
pub enum Pane {
    /// Leaf pane containing tabs.
    Leaf {
        /// Tabs in this pane.
        tabs: Vec<Tab>,
        /// Active tab index.
        active_tab: usize,
        /// Pane bounds.
        bounds: Rect,
        /// Unique pane ID.
        id: u32,
    },
    /// Split pane containing two child panes.
    Split {
        /// Split direction.
        direction: SplitDirection,
        /// First child pane.
        first: Box<Pane>,
        /// Second child pane.
        second: Box<Pane>,
        /// Split ratio (0.0 to 1.0, position of divider).
        ratio: f64,
        /// Pane bounds.
        bounds: Rect,
    },
}

impl Pane {
    /// Create a new leaf pane with a single tab.
    pub fn new_leaf(path: PathBuf, bounds: Rect, id: u32) -> Self {
        let tab = Tab::new(path, bounds);
        Pane::Leaf {
            tabs: vec![tab],
            active_tab: 0,
            bounds,
            id,
        }
    }

    /// Get pane bounds.
    pub fn bounds(&self) -> Rect {
        match self {
            Pane::Leaf { bounds, .. } => *bounds,
            Pane::Split { bounds, .. } => *bounds,
        }
    }

    /// Update pane bounds.
    pub fn set_bounds(&mut self, new_bounds: Rect) {
        match self {
            Pane::Leaf { bounds, tabs, .. } => {
                *bounds = new_bounds;
                for tab in tabs {
                    tab.set_bounds(new_bounds);
                }
            }
            Pane::Split {
                direction,
                first,
                second,
                ratio,
                bounds,
            } => {
                *bounds = new_bounds;

                // Calculate child bounds based on split direction and ratio
                let (first_bounds, second_bounds) = match direction {
                    SplitDirection::Horizontal => {
                        let first_width = ((new_bounds.width as f64 - DIVIDER_SIZE as f64) * *ratio) as u32;
                        let second_width = new_bounds.width - first_width - DIVIDER_SIZE;
                        (
                            Rect::new(new_bounds.x, new_bounds.y, first_width, new_bounds.height),
                            Rect::new(
                                new_bounds.x + first_width as i32 + DIVIDER_SIZE as i32,
                                new_bounds.y,
                                second_width,
                                new_bounds.height,
                            ),
                        )
                    }
                    SplitDirection::Vertical => {
                        let first_height = ((new_bounds.height as f64 - DIVIDER_SIZE as f64) * *ratio) as u32;
                        let second_height = new_bounds.height - first_height - DIVIDER_SIZE;
                        (
                            Rect::new(new_bounds.x, new_bounds.y, new_bounds.width, first_height),
                            Rect::new(
                                new_bounds.x,
                                new_bounds.y + first_height as i32 + DIVIDER_SIZE as i32,
                                new_bounds.width,
                                second_height,
                            ),
                        )
                    }
                };

                first.set_bounds(first_bounds);
                second.set_bounds(second_bounds);
            }
        }
    }

    /// Check if a point is on the split divider. Returns the pane if so.
    pub fn divider_at(&self, pos: Point) -> Option<&Pane> {
        match self {
            Pane::Leaf { .. } => None,
            Pane::Split {
                first,
                second,
                ..
            } => {
                let divider_bounds = self.divider_bounds();
                if let Some(db) = divider_bounds {
                    if db.contains_point(pos) {
                        return Some(self);
                    }
                }

                // Check children
                first.divider_at(pos).or_else(|| second.divider_at(pos))
            }
        }
    }

    /// Find split divider at point, returning a path to it.
    /// Path is a list of booleans: true = first child, false = second child.
    pub fn split_divider_at(&self, pos: Point) -> Option<Vec<bool>> {
        match self {
            Pane::Leaf { .. } => None,
            Pane::Split { first, second, .. } => {
                // Check if pos is on this split's divider
                if let Some(db) = self.divider_bounds() {
                    if db.contains_point(pos) {
                        return Some(vec![]); // Empty path = this split
                    }
                }

                // Check children
                if let Some(mut path) = first.split_divider_at(pos) {
                    path.insert(0, true);
                    return Some(path);
                }
                if let Some(mut path) = second.split_divider_at(pos) {
                    path.insert(0, false);
                    return Some(path);
                }

                None
            }
        }
    }

    /// Adjust split ratio at the given path based on mouse position.
    pub fn adjust_split_at(&mut self, path: &[bool], pos: Point) {
        if path.is_empty() {
            // Adjust this split
            if let Pane::Split { direction, ratio, bounds, .. } = self {
                let new_ratio = match direction {
                    SplitDirection::Horizontal => {
                        let relative_x = (pos.x - bounds.x) as f64;
                        let total_width = bounds.width as f64;
                        (relative_x / total_width).clamp(0.1, 0.9)
                    }
                    SplitDirection::Vertical => {
                        let relative_y = (pos.y - bounds.y) as f64;
                        let total_height = bounds.height as f64;
                        (relative_y / total_height).clamp(0.1, 0.9)
                    }
                };
                *ratio = new_ratio;

                // Recalculate child bounds
                let current_bounds = *bounds;
                self.set_bounds(current_bounds);
            }
        } else {
            // Navigate to child
            if let Pane::Split { first, second, .. } = self {
                if path[0] {
                    first.adjust_split_at(&path[1..], pos);
                } else {
                    second.adjust_split_at(&path[1..], pos);
                }
            }
        }
    }

    /// Get split direction for this pane.
    pub fn split_direction(&self) -> Option<SplitDirection> {
        match self {
            Pane::Leaf { .. } => None,
            Pane::Split { direction, .. } => Some(*direction),
        }
    }

    /// Get split direction at the given path.
    pub fn split_direction_at(&self, path: &[bool]) -> Option<SplitDirection> {
        if path.is_empty() {
            self.split_direction()
        } else {
            match self {
                Pane::Leaf { .. } => None,
                Pane::Split { first, second, .. } => {
                    if path[0] {
                        first.split_direction_at(&path[1..])
                    } else {
                        second.split_direction_at(&path[1..])
                    }
                }
            }
        }
    }

    /// Get divider bounds for a split pane.
    fn divider_bounds(&self) -> Option<Rect> {
        match self {
            Pane::Leaf { .. } => None,
            Pane::Split {
                direction,
                first,
                bounds,
                ..
            } => {
                let first_bounds = first.bounds();
                match direction {
                    SplitDirection::Horizontal => Some(Rect::new(
                        first_bounds.x + first_bounds.width as i32,
                        bounds.y,
                        DIVIDER_SIZE,
                        bounds.height,
                    )),
                    SplitDirection::Vertical => Some(Rect::new(
                        bounds.x,
                        first_bounds.y + first_bounds.height as i32,
                        bounds.width,
                        DIVIDER_SIZE,
                    )),
                }
            }
        }
    }

    /// Find the leaf pane containing the given point.
    pub fn leaf_at(&self, pos: Point) -> Option<&Pane> {
        match self {
            Pane::Leaf { bounds, .. } => {
                if bounds.contains_point(pos) {
                    Some(self)
                } else {
                    None
                }
            }
            Pane::Split { first, second, .. } => {
                first.leaf_at(pos).or_else(|| second.leaf_at(pos))
            }
        }
    }

    /// Find the leaf pane containing the given point (mutable).
    pub fn leaf_at_mut(&mut self, pos: Point) -> Option<&mut Pane> {
        match self {
            Pane::Leaf { bounds, .. } => {
                if bounds.contains_point(pos) {
                    Some(self)
                } else {
                    None
                }
            }
            Pane::Split { first, second, .. } => {
                if first.bounds().contains_point(pos) {
                    first.leaf_at_mut(pos)
                } else {
                    second.leaf_at_mut(pos)
                }
            }
        }
    }

    /// Find leaf pane by ID.
    pub fn leaf_by_id(&self, target_id: u32) -> Option<&Pane> {
        match self {
            Pane::Leaf { id, .. } => {
                if *id == target_id {
                    Some(self)
                } else {
                    None
                }
            }
            Pane::Split { first, second, .. } => {
                first.leaf_by_id(target_id).or_else(|| second.leaf_by_id(target_id))
            }
        }
    }

    /// Find leaf pane by ID (mutable).
    pub fn leaf_by_id_mut(&mut self, target_id: u32) -> Option<&mut Pane> {
        match self {
            Pane::Leaf { id, .. } => {
                if *id == target_id {
                    Some(self)
                } else {
                    None
                }
            }
            Pane::Split { first, second, .. } => {
                if first.leaf_by_id(target_id).is_some() {
                    first.leaf_by_id_mut(target_id)
                } else {
                    second.leaf_by_id_mut(target_id)
                }
            }
        }
    }

    /// Get all leaf pane IDs.
    pub fn leaf_ids(&self) -> Vec<u32> {
        match self {
            Pane::Leaf { id, .. } => vec![*id],
            Pane::Split { first, second, .. } => {
                let mut ids = first.leaf_ids();
                ids.extend(second.leaf_ids());
                ids
            }
        }
    }

    /// Split this pane. Only works on leaf panes.
    /// Returns the new pane ID if successful.
    pub fn split(&mut self, direction: SplitDirection, new_path: PathBuf, new_id: u32) -> Option<u32> {
        let current_bounds = self.bounds();

        match self {
            Pane::Leaf { tabs, active_tab, bounds, id } => {
                // Create new leaf from current state
                let first_pane = Pane::Leaf {
                    tabs: std::mem::take(tabs),
                    active_tab: *active_tab,
                    bounds: *bounds,
                    id: *id,
                };

                // Create second leaf with new tab
                let second_pane = Pane::new_leaf(new_path, *bounds, new_id);

                // Replace self with split
                *self = Pane::Split {
                    direction,
                    first: Box::new(first_pane),
                    second: Box::new(second_pane),
                    ratio: 0.5,
                    bounds: current_bounds,
                };

                // Recalculate bounds
                self.set_bounds(current_bounds);

                Some(new_id)
            }
            Pane::Split { .. } => None, // Can't split a split directly
        }
    }

    /// Render the pane.
    pub fn render(&self, renderer: &Renderer, focused_id: Option<u32>) -> anyhow::Result<()> {
        match self {
            Pane::Leaf { tabs, active_tab, bounds, id } => {
                if let Some(tab) = tabs.get(*active_tab) {
                    tab.render(renderer)?;
                }

                // Draw focus indicator if this pane is focused
                if focused_id == Some(*id) {
                    let theme = renderer.theme();
                    renderer.stroke_rect(*bounds, theme.selection_background, 2.0)?;
                }
            }
            Pane::Split {
                direction,
                first,
                second,
                ..
            } => {
                first.render(renderer, focused_id)?;
                second.render(renderer, focused_id)?;

                // Draw divider
                if let Some(divider) = self.divider_bounds() {
                    let theme = renderer.theme();
                    renderer.fill_rect(divider, theme.border)?;
                }
            }
        }

        Ok(())
    }

    // === Tab operations (for leaf panes) ===

    /// Get active tab (for leaf panes).
    pub fn active_tab(&self) -> Option<&Tab> {
        match self {
            Pane::Leaf { tabs, active_tab, .. } => tabs.get(*active_tab),
            Pane::Split { .. } => None,
        }
    }

    /// Get active tab (mutable, for leaf panes).
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        match self {
            Pane::Leaf { tabs, active_tab, .. } => tabs.get_mut(*active_tab),
            Pane::Split { .. } => None,
        }
    }

    /// Add a new tab to a leaf pane.
    pub fn add_tab(&mut self, path: PathBuf) {
        if let Pane::Leaf { tabs, active_tab, bounds, .. } = self {
            let tab = Tab::new(path, *bounds);
            tabs.push(tab);
            *active_tab = tabs.len() - 1;
        }
    }

    /// Close the active tab. Returns true if pane should be removed (no tabs left).
    pub fn close_active_tab(&mut self) -> bool {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if tabs.len() <= 1 {
                return true; // Pane should be removed
            }

            tabs.remove(*active_tab);
            if *active_tab >= tabs.len() {
                *active_tab = tabs.len() - 1;
            }
        }
        false
    }

    /// Close tab at index. Returns true if pane should be removed.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if index >= tabs.len() {
                return false;
            }

            if tabs.len() <= 1 {
                return true; // Pane should be removed
            }

            tabs.remove(index);
            if *active_tab >= tabs.len() {
                *active_tab = tabs.len() - 1;
            } else if *active_tab > index {
                *active_tab -= 1;
            }
        }
        false
    }

    /// Set active tab index.
    pub fn set_active_tab(&mut self, index: usize) {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if index < tabs.len() {
                *active_tab = index;
            }
        }
    }

    /// Get tab count.
    pub fn tab_count(&self) -> usize {
        match self {
            Pane::Leaf { tabs, .. } => tabs.len(),
            Pane::Split { .. } => 0,
        }
    }

    /// Get active tab index.
    pub fn active_tab_index(&self) -> usize {
        match self {
            Pane::Leaf { active_tab, .. } => *active_tab,
            Pane::Split { .. } => 0,
        }
    }

    /// Get tabs (for tab bar display).
    pub fn tabs(&self) -> &[Tab] {
        match self {
            Pane::Leaf { tabs, .. } => tabs,
            Pane::Split { .. } => &[],
        }
    }

    /// Cycle to next tab.
    pub fn next_tab(&mut self) {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if !tabs.is_empty() {
                *active_tab = (*active_tab + 1) % tabs.len();
            }
        }
    }

    /// Cycle to previous tab.
    pub fn prev_tab(&mut self) {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if !tabs.is_empty() {
                *active_tab = if *active_tab == 0 {
                    tabs.len() - 1
                } else {
                    *active_tab - 1
                };
            }
        }
    }

    /// Get pane ID (for leaf panes).
    pub fn id(&self) -> Option<u32> {
        match self {
            Pane::Leaf { id, .. } => Some(*id),
            Pane::Split { .. } => None,
        }
    }

    // === Delegation to active tab for resize/drag ===

    /// Check if mouse is on a column divider (for list view).
    pub fn column_divider_at(&self, pos: Point) -> Option<usize> {
        match self {
            Pane::Leaf { tabs, active_tab, .. } => {
                tabs.get(*active_tab).and_then(|t| t.divider_at(pos))
            }
            Pane::Split { .. } => None,
        }
    }

    /// Start column resize.
    pub fn start_resize(&mut self, divider: usize) {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if let Some(tab) = tabs.get_mut(*active_tab) {
                tab.start_resize(divider);
            }
        }
    }

    /// Stop column resize.
    pub fn stop_resize(&mut self) {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if let Some(tab) = tabs.get_mut(*active_tab) {
                tab.stop_resize();
            }
        }
    }

    /// Check if resizing is in progress.
    pub fn is_resizing(&self) -> bool {
        match self {
            Pane::Leaf { tabs, active_tab, .. } => {
                tabs.get(*active_tab).map(|t| t.is_resizing()).unwrap_or(false)
            }
            Pane::Split { .. } => false,
        }
    }

    /// Check if dragging (rubber band selection) is in progress.
    pub fn is_dragging(&self) -> bool {
        match self {
            Pane::Leaf { tabs, active_tab, .. } => {
                tabs.get(*active_tab).map(|t| t.is_dragging()).unwrap_or(false)
            }
            Pane::Split { .. } => false,
        }
    }

    /// Stop drag (rubber band selection).
    pub fn stop_drag(&mut self) {
        if let Pane::Leaf { tabs, active_tab, .. } = self {
            if let Some(tab) = tabs.get_mut(*active_tab) {
                tab.stop_drag();
            }
        }
    }

    /// Get the center point of a leaf pane.
    pub fn center(&self) -> Option<Point> {
        match self {
            Pane::Leaf { bounds, .. } => {
                Some(Point::new(
                    bounds.x + bounds.width as i32 / 2,
                    bounds.y + bounds.height as i32 / 2,
                ))
            }
            Pane::Split { .. } => None,
        }
    }

    /// Find pane to the left of given ID.
    pub fn pane_left_of(&self, current_id: u32) -> Option<u32> {
        self.find_adjacent_pane(current_id, |current, candidate| {
            let cc = current.center()?;
            let pc = candidate.center()?;
            // Candidate must be to the left
            if pc.x >= cc.x {
                return None;
            }
            // Prefer candidates closer in y, then closer in x
            Some(((cc.y - pc.y).abs() * 1000 + (cc.x - pc.x).abs()) as u32)
        })
    }

    /// Find pane to the right of given ID.
    pub fn pane_right_of(&self, current_id: u32) -> Option<u32> {
        self.find_adjacent_pane(current_id, |current, candidate| {
            let cc = current.center()?;
            let pc = candidate.center()?;
            // Candidate must be to the right
            if pc.x <= cc.x {
                return None;
            }
            Some(((cc.y - pc.y).abs() * 1000 + (pc.x - cc.x).abs()) as u32)
        })
    }

    /// Find pane above given ID.
    pub fn pane_above(&self, current_id: u32) -> Option<u32> {
        self.find_adjacent_pane(current_id, |current, candidate| {
            let cc = current.center()?;
            let pc = candidate.center()?;
            // Candidate must be above
            if pc.y >= cc.y {
                return None;
            }
            Some(((cc.x - pc.x).abs() * 1000 + (cc.y - pc.y).abs()) as u32)
        })
    }

    /// Find pane below given ID.
    pub fn pane_below(&self, current_id: u32) -> Option<u32> {
        self.find_adjacent_pane(current_id, |current, candidate| {
            let cc = current.center()?;
            let pc = candidate.center()?;
            // Candidate must be below
            if pc.y <= cc.y {
                return None;
            }
            Some(((cc.x - pc.x).abs() * 1000 + (pc.y - cc.y).abs()) as u32)
        })
    }

    /// Find adjacent pane using a scoring function.
    /// Lower score = better match. None = not a valid candidate.
    fn find_adjacent_pane<F>(&self, current_id: u32, score_fn: F) -> Option<u32>
    where
        F: Fn(&Pane, &Pane) -> Option<u32>,
    {
        let current = self.leaf_by_id(current_id)?;
        let all_ids = self.leaf_ids();

        let mut best_id = None;
        let mut best_score = u32::MAX;

        for id in all_ids {
            if id == current_id {
                continue;
            }
            if let Some(candidate) = self.leaf_by_id(id) {
                if let Some(score) = score_fn(current, candidate) {
                    if score < best_score {
                        best_score = score;
                        best_id = Some(id);
                    }
                }
            }
        }

        best_id
    }

    /// Get the first leaf ID in this pane tree.
    pub fn first_leaf_id(&self) -> Option<u32> {
        match self {
            Pane::Leaf { id, .. } => Some(*id),
            Pane::Split { first, .. } => first.first_leaf_id(),
        }
    }

    /// Remove a pane by ID. Returns the ID of a sibling pane to focus on.
    /// Returns None if the pane is the root leaf (can't close the last pane).
    pub fn remove_pane(&mut self, target_id: u32) -> Option<u32> {
        match self {
            Pane::Leaf { id, .. } => {
                // Can't remove the root leaf - it's the only pane
                if *id == target_id {
                    None
                } else {
                    None // Target not found
                }
            }
            Pane::Split { first, second, bounds, .. } => {
                // Check if first child is the target leaf
                if let Some(first_id) = first.id() {
                    if first_id == target_id {
                        // Replace self with second child
                        let sibling_id = second.first_leaf_id();
                        let current_bounds = *bounds;
                        let replacement = std::mem::replace(
                            second.as_mut(),
                            Pane::new_leaf(PathBuf::new(), Rect::new(0, 0, 0, 0), 0),
                        );
                        *self = replacement;
                        self.set_bounds(current_bounds);
                        return sibling_id;
                    }
                }

                // Check if second child is the target leaf
                if let Some(second_id) = second.id() {
                    if second_id == target_id {
                        // Replace self with first child
                        let sibling_id = first.first_leaf_id();
                        let current_bounds = *bounds;
                        let replacement = std::mem::replace(
                            first.as_mut(),
                            Pane::new_leaf(PathBuf::new(), Rect::new(0, 0, 0, 0), 0),
                        );
                        *self = replacement;
                        self.set_bounds(current_bounds);
                        return sibling_id;
                    }
                }

                // Recurse into children
                if first.leaf_by_id(target_id).is_some() {
                    first.remove_pane(target_id)
                } else {
                    second.remove_pane(target_id)
                }
            }
        }
    }
}
