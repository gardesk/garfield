//! Asynchronous image preview loading.
//!
//! This module provides non-blocking image loading and scaling to prevent UI lag
//! when selecting image files for preview.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Request to load an image preview.
#[derive(Debug, Clone)]
pub struct ImagePreviewRequest {
    /// Path to load.
    pub path: PathBuf,
    /// Maximum width for the preview.
    pub max_width: u32,
    /// Maximum height for the preview.
    pub max_height: u32,
    /// Request ID for matching responses.
    pub request_id: u64,
}

/// Result of an image preview load operation.
#[derive(Debug)]
pub struct ImagePreviewResult {
    /// Path that was loaded.
    pub path: PathBuf,
    /// Loaded image data (RGBA format), or None if load failed.
    pub image: Option<ImagePreview>,
    /// Request ID for matching.
    pub request_id: u64,
}

/// Loaded and scaled image preview.
#[derive(Debug, Clone)]
pub struct ImagePreview {
    /// RGBA pixel data.
    pub data: Vec<u8>,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
}

/// Manages asynchronous image preview loading with a worker thread.
pub struct ImagePreviewLoader {
    /// Sender to submit load requests.
    request_tx: Sender<ImagePreviewRequest>,
    /// Receiver for completed loads.
    result_rx: Receiver<ImagePreviewResult>,
    /// Worker thread handle.
    _worker: JoinHandle<()>,
    /// Current request ID counter.
    next_request_id: u64,
    /// ID of the most recent request (for ignoring stale results).
    current_request_id: u64,
}

impl ImagePreviewLoader {
    /// Create a new image preview loader with a background worker thread.
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ImagePreviewRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ImagePreviewResult>();

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
    fn worker_loop(request_rx: Receiver<ImagePreviewRequest>, result_tx: Sender<ImagePreviewResult>) {
        while let Ok(request) = request_rx.recv() {
            let image = Self::load_and_scale(&request.path, request.max_width, request.max_height);

            // Send result back
            let _ = result_tx.send(ImagePreviewResult {
                path: request.path,
                image,
                request_id: request.request_id,
            });
        }
    }

    /// Load and scale an image to fit within the given dimensions.
    fn load_and_scale(path: &PathBuf, max_width: u32, max_height: u32) -> Option<ImagePreview> {
        use image::GenericImageView;

        // Load the image
        let img = image::open(path).ok()?;

        let (orig_width, orig_height) = img.dimensions();

        // Calculate scale to fit within max dimensions while preserving aspect ratio
        let scale_x = max_width as f64 / orig_width as f64;
        let scale_y = max_height as f64 / orig_height as f64;
        let scale = scale_x.min(scale_y).min(1.0); // Don't upscale

        let new_width = (orig_width as f64 * scale) as u32;
        let new_height = (orig_height as f64 * scale) as u32;

        // Resize if needed
        let resized = if scale < 1.0 {
            img.resize(new_width, new_height, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        // Convert to RGBA
        let rgba = resized.to_rgba8();
        let (width, height) = rgba.dimensions();

        Some(ImagePreview {
            data: rgba.into_raw(),
            width,
            height,
        })
    }

    /// Request loading an image preview. Returns the request ID.
    pub fn load(&mut self, path: PathBuf, max_width: u32, max_height: u32) -> u64 {
        self.next_request_id += 1;
        self.current_request_id = self.next_request_id;

        let request = ImagePreviewRequest {
            path,
            max_width,
            max_height,
            request_id: self.current_request_id,
        };

        let _ = self.request_tx.send(request);
        self.current_request_id
    }

    /// Poll for a completed preview load. Returns None if no result ready.
    pub fn poll(&mut self) -> Option<ImagePreviewResult> {
        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    if result.request_id == self.current_request_id {
                        return Some(result);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => return None,
            }
        }
    }

    /// Cancel any pending requests.
    pub fn cancel(&mut self) {
        self.current_request_id = 0;
    }
}

impl Default for ImagePreviewLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a file extension is a supported image format.
pub fn is_supported_image(extension: Option<&str>) -> bool {
    match extension {
        Some(ext) => matches!(
            ext.to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "ico"
        ),
        None => false,
    }
}
