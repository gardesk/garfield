//! Navigation history for back/forward support.

use std::path::PathBuf;

/// Navigation history with back/forward support.
#[derive(Debug, Clone)]
pub struct History {
    /// Past entries (for going back).
    back_stack: Vec<PathBuf>,
    /// Future entries (for going forward).
    forward_stack: Vec<PathBuf>,
    /// Current location.
    current: PathBuf,
    /// Maximum history size.
    max_size: usize,
}

impl History {
    /// Create a new history starting at the given path.
    pub fn new(start: PathBuf) -> Self {
        Self {
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            current: start,
            max_size: 100,
        }
    }

    /// Get the current location.
    pub fn current(&self) -> &PathBuf {
        &self.current
    }

    /// Navigate to a new path, clearing forward history.
    pub fn navigate(&mut self, path: PathBuf) {
        if path == self.current {
            return;
        }

        // Push current to back stack
        self.back_stack.push(self.current.clone());

        // Trim if too large
        if self.back_stack.len() > self.max_size {
            self.back_stack.remove(0);
        }

        // Clear forward stack (new navigation branch)
        self.forward_stack.clear();

        self.current = path;
    }

    /// Go back in history. Returns the new current path if successful.
    pub fn go_back(&mut self) -> Option<&PathBuf> {
        if let Some(prev) = self.back_stack.pop() {
            self.forward_stack.push(self.current.clone());
            self.current = prev;
            Some(&self.current)
        } else {
            None
        }
    }

    /// Go forward in history. Returns the new current path if successful.
    pub fn go_forward(&mut self) -> Option<&PathBuf> {
        if let Some(next) = self.forward_stack.pop() {
            self.back_stack.push(self.current.clone());
            self.current = next;
            Some(&self.current)
        } else {
            None
        }
    }

    /// Check if we can go back.
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    /// Check if we can go forward.
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }
}
