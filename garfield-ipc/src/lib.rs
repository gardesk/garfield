//! Shared IPC protocol types for garfield.
//!
//! This crate defines the request/response types used for communication
//! between garfield and garfieldctl, as well as with other gar components.

use serde::{Deserialize, Serialize};

/// IPC request from client to garfield.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Open a directory in garfield.
    Open {
        path: String,
        /// Open in new tab instead of current view.
        #[serde(default)]
        new_tab: bool,
    },
    /// Query current directory.
    CurrentDir,
    /// Query garfield status.
    Status,
    /// Request file picker dialog.
    FilePicker {
        /// Dialog title.
        title: Option<String>,
        /// Starting directory.
        start_dir: Option<String>,
        /// File type filters (e.g., "*.rs", "*.txt").
        filters: Vec<String>,
        /// Allow selecting multiple files.
        #[serde(default)]
        multiple: bool,
    },
    /// Request folder picker dialog.
    FolderPicker {
        /// Dialog title.
        title: Option<String>,
        /// Starting directory.
        start_dir: Option<String>,
    },
    /// Quit garfield.
    Quit,
}

/// IPC response from garfield to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Whether the command succeeded.
    pub success: bool,
    /// Response data (command-specific).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    /// Create a successful response with no data.
    pub fn ok() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
        }
    }

    /// Create a successful response with data.
    pub fn ok_with_data(data: impl Serialize) -> Self {
        Self {
            success: true,
            data: serde_json::to_value(data).ok(),
            error: None,
        }
    }

    /// Create an error response.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Status information returned by the Status command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    /// Current working directory.
    pub current_dir: String,
    /// Number of open tabs.
    pub tab_count: usize,
    /// Number of selected files.
    pub selection_count: usize,
}

/// Result of a file picker dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickerResult {
    /// Selected file paths (empty if cancelled).
    pub paths: Vec<String>,
    /// Whether the dialog was cancelled.
    pub cancelled: bool,
}

/// Default socket path for garfield IPC.
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("garfield.sock")
}
