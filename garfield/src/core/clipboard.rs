//! Clipboard state for file operations.

use std::path::PathBuf;

/// Operation type for clipboard contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOperation {
    /// Files will be copied.
    Copy,
    /// Files will be moved (cut).
    Cut,
}

/// Clipboard holding files for copy/cut operations.
#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    /// Files in the clipboard.
    files: Vec<PathBuf>,
    /// Operation to perform.
    operation: Option<ClipboardOperation>,
}

impl Clipboard {
    /// Create a new empty clipboard.
    pub fn new() -> Self {
        Self::default()
    }

    /// Copy files to clipboard.
    pub fn copy(&mut self, files: Vec<PathBuf>) {
        self.files = files;
        self.operation = Some(ClipboardOperation::Copy);
    }

    /// Cut files to clipboard.
    pub fn cut(&mut self, files: Vec<PathBuf>) {
        self.files = files;
        self.operation = Some(ClipboardOperation::Cut);
    }

    /// Clear the clipboard.
    pub fn clear(&mut self) {
        self.files.clear();
        self.operation = None;
    }

    /// Get files in the clipboard.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Get the operation type.
    pub fn operation(&self) -> Option<ClipboardOperation> {
        self.operation
    }

    /// Check if clipboard has files.
    pub fn has_files(&self) -> bool {
        !self.files.is_empty() && self.operation.is_some()
    }

    /// Check if this is a cut operation.
    pub fn is_cut(&self) -> bool {
        self.operation == Some(ClipboardOperation::Cut)
    }

    /// Get status text for display.
    pub fn status_text(&self) -> Option<String> {
        if !self.has_files() {
            return None;
        }

        let count = self.files.len();
        let op = match self.operation {
            Some(ClipboardOperation::Copy) => "copied",
            Some(ClipboardOperation::Cut) => "cut",
            None => return None,
        };

        Some(if count == 1 {
            format!("1 item {}", op)
        } else {
            format!("{} items {}", count, op)
        })
    }

    /// Take files from clipboard (consumes for cut operations).
    pub fn take(&mut self) -> Option<(Vec<PathBuf>, ClipboardOperation)> {
        if !self.has_files() {
            return None;
        }

        let files = std::mem::take(&mut self.files);
        let op = self.operation.take()?;

        // For copy, restore the files so they can be pasted again
        if op == ClipboardOperation::Copy {
            self.files = files.clone();
            self.operation = Some(op);
        }

        Some((files, op))
    }
}
