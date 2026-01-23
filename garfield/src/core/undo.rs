//! Undo/redo system for file operations.

use std::path::PathBuf;

/// Maximum number of operations to keep in history.
const MAX_HISTORY: usize = 100;

/// A file operation that can be undone.
#[derive(Debug, Clone)]
pub enum FileOperation {
    /// Copy files (sources, created destinations).
    Copy {
        sources: Vec<PathBuf>,
        destinations: Vec<PathBuf>,
    },
    /// Move files (sources, destinations).
    Move {
        sources: Vec<PathBuf>,
        destinations: Vec<PathBuf>,
    },
    /// Trash files (original paths, trash entry names).
    Trash {
        originals: Vec<PathBuf>,
        trash_names: Vec<String>,
    },
    /// Rename file (original path, new path).
    Rename {
        original: PathBuf,
        renamed: PathBuf,
    },
    /// Create directory (path).
    CreateDir {
        path: PathBuf,
    },
}

impl FileOperation {
    /// Get a human-readable description of this operation.
    pub fn description(&self) -> String {
        match self {
            FileOperation::Copy { sources, .. } => {
                if sources.len() == 1 {
                    format!("Copy '{}'", sources[0].file_name().unwrap_or_default().to_string_lossy())
                } else {
                    format!("Copy {} items", sources.len())
                }
            }
            FileOperation::Move { sources, .. } => {
                if sources.len() == 1 {
                    format!("Move '{}'", sources[0].file_name().unwrap_or_default().to_string_lossy())
                } else {
                    format!("Move {} items", sources.len())
                }
            }
            FileOperation::Trash { originals, .. } => {
                if originals.len() == 1 {
                    format!("Trash '{}'", originals[0].file_name().unwrap_or_default().to_string_lossy())
                } else {
                    format!("Trash {} items", originals.len())
                }
            }
            FileOperation::Rename { original, renamed } => {
                format!(
                    "Rename '{}' to '{}'",
                    original.file_name().unwrap_or_default().to_string_lossy(),
                    renamed.file_name().unwrap_or_default().to_string_lossy()
                )
            }
            FileOperation::CreateDir { path } => {
                format!("Create folder '{}'", path.file_name().unwrap_or_default().to_string_lossy())
            }
        }
    }
}

/// Manages undo/redo stacks for file operations.
#[derive(Debug, Default)]
pub struct UndoStack {
    /// Operations that can be undone (most recent last).
    undo_stack: Vec<FileOperation>,
    /// Operations that can be redone (most recent last).
    redo_stack: Vec<FileOperation>,
}

impl UndoStack {
    /// Create a new undo stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an operation onto the undo stack.
    /// Clears the redo stack since the history has diverged.
    pub fn push(&mut self, op: FileOperation) {
        self.undo_stack.push(op);
        self.redo_stack.clear();

        // Limit history size
        if self.undo_stack.len() > MAX_HISTORY {
            self.undo_stack.remove(0);
        }
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Pop an operation from the undo stack for undoing.
    /// The operation should be performed, then moved to redo.
    pub fn pop_undo(&mut self) -> Option<FileOperation> {
        self.undo_stack.pop()
    }

    /// Push an operation onto the redo stack (after undoing).
    pub fn push_redo(&mut self, op: FileOperation) {
        self.redo_stack.push(op);
    }

    /// Pop an operation from the redo stack for redoing.
    pub fn pop_redo(&mut self) -> Option<FileOperation> {
        self.redo_stack.pop()
    }

    /// Get description of the next undo operation.
    pub fn undo_description(&self) -> Option<String> {
        self.undo_stack.last().map(|op| op.description())
    }

    /// Get description of the next redo operation.
    pub fn redo_description(&self) -> Option<String> {
        self.redo_stack.last().map(|op| op.description())
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
