//! Asynchronous preview loading for directory contents.
//!
//! This module provides non-blocking directory loading to prevent UI lag
//! when selecting directories with many files.

use crate::core::{read_directory, sort_entries, FileEntry, SortDirection, SortOrder};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Request to load a directory preview.
#[derive(Debug, Clone)]
pub struct PreviewRequest {
    /// Path to load.
    pub path: PathBuf,
    /// Sort order to apply.
    pub sort_order: SortOrder,
    /// Sort direction to apply.
    pub sort_direction: SortDirection,
    /// Request ID for matching responses.
    pub request_id: u64,
}

/// Result of a preview load operation.
#[derive(Debug)]
pub struct PreviewResult {
    /// Path that was loaded.
    pub path: PathBuf,
    /// Loaded entries (None if load failed).
    pub entries: Option<Vec<FileEntry>>,
    /// Request ID for matching.
    pub request_id: u64,
}

/// Manages asynchronous preview loading with a worker thread.
pub struct PreviewLoader {
    /// Sender to submit load requests.
    request_tx: Sender<PreviewRequest>,
    /// Receiver for completed loads.
    result_rx: Receiver<PreviewResult>,
    /// Worker thread handle.
    _worker: JoinHandle<()>,
    /// Current request ID counter.
    next_request_id: u64,
    /// ID of the most recent request (for ignoring stale results).
    current_request_id: u64,
}

impl PreviewLoader {
    /// Create a new preview loader with a background worker thread.
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<PreviewRequest>();
        let (result_tx, result_rx) = mpsc::channel::<PreviewResult>();

        let worker = thread::spawn(move || {
            Self::worker_loop(request_rx, result_tx);
        });

        Self {
            request_tx,
            result_rx,
            _worker: worker,
            next_request_id: 0,
            current_request_id: 0,
        }
    }

    /// Worker thread loop - processes load requests.
    fn worker_loop(request_rx: Receiver<PreviewRequest>, result_tx: Sender<PreviewResult>) {
        while let Ok(request) = request_rx.recv() {
            // Load the directory
            let entries = read_directory(&request.path).ok().map(|mut entries| {
                sort_entries(&mut entries, request.sort_order, request.sort_direction);
                entries
            });

            // Send result back (ignore send errors - main thread may have dropped receiver)
            let _ = result_tx.send(PreviewResult {
                path: request.path,
                entries,
                request_id: request.request_id,
            });
        }
    }

    /// Request loading a directory preview. Returns the request ID.
    pub fn load(&mut self, path: PathBuf, sort_order: SortOrder, sort_direction: SortDirection) -> u64 {
        self.next_request_id += 1;
        self.current_request_id = self.next_request_id;

        let request = PreviewRequest {
            path,
            sort_order,
            sort_direction,
            request_id: self.current_request_id,
        };

        // Ignore send errors - worker thread may have panicked
        let _ = self.request_tx.send(request);

        self.current_request_id
    }

    /// Poll for a completed preview load. Returns None if no result ready.
    /// Only returns results for the most recent request (ignores stale results).
    pub fn poll(&mut self) -> Option<PreviewResult> {
        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    // Only return if this is the current request
                    if result.request_id == self.current_request_id {
                        return Some(result);
                    }
                    // Otherwise, discard stale result and keep polling
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => return None,
            }
        }
    }

    /// Check if there's a pending load request.
    pub fn is_loading(&self) -> bool {
        // We're loading if we've sent a request but haven't received a matching result
        self.current_request_id > 0
    }

    /// Cancel any pending requests by invalidating the current request ID.
    pub fn cancel(&mut self) {
        self.current_request_id = 0;
    }
}

impl Default for PreviewLoader {
    fn default() -> Self {
        Self::new()
    }
}
