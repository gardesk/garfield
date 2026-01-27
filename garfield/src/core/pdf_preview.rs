//! Asynchronous PDF preview loading.
//!
//! This module provides non-blocking PDF rendering to prevent UI lag
//! when selecting PDF files for preview.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Request to load a PDF preview.
#[derive(Debug, Clone)]
pub struct PdfPreviewRequest {
    /// Path to load.
    pub path: PathBuf,
    /// Maximum width for the preview.
    pub max_width: u32,
    /// Maximum height for the preview.
    pub max_height: u32,
    /// Request ID for matching responses.
    pub request_id: u64,
}

/// Result of a PDF preview load operation.
#[derive(Debug)]
pub struct PdfPreviewResult {
    /// Path that was loaded.
    pub path: PathBuf,
    /// Loaded image data (RGBA format), or None if load failed.
    pub image: Option<PdfPreview>,
    /// Request ID for matching.
    pub request_id: u64,
}

/// Loaded and rendered PDF preview (first page).
#[derive(Debug, Clone)]
pub struct PdfPreview {
    /// RGBA pixel data.
    pub data: Vec<u8>,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
    /// Total number of pages in the PDF.
    pub page_count: usize,
}

/// Manages asynchronous PDF preview loading with a worker thread.
pub struct PdfPreviewLoader {
    /// Sender to submit load requests.
    request_tx: Sender<PdfPreviewRequest>,
    /// Receiver for completed loads.
    result_rx: Receiver<PdfPreviewResult>,
    /// Worker thread handle.
    _worker: JoinHandle<()>,
    /// Current request ID counter.
    next_request_id: u64,
    /// ID of the most recent request (for ignoring stale results).
    current_request_id: u64,
}

impl PdfPreviewLoader {
    /// Create a new PDF preview loader with a background worker thread.
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<PdfPreviewRequest>();
        let (result_tx, result_rx) = mpsc::channel::<PdfPreviewResult>();

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
    fn worker_loop(request_rx: Receiver<PdfPreviewRequest>, result_tx: Sender<PdfPreviewResult>) {
        while let Ok(request) = request_rx.recv() {
            let image = Self::render_first_page(&request.path, request.max_width, request.max_height);

            // Send result back
            let _ = result_tx.send(PdfPreviewResult {
                path: request.path,
                image,
                request_id: request.request_id,
            });
        }
    }

    /// Render the first page of a PDF to an image.
    fn render_first_page(path: &PathBuf, max_width: u32, max_height: u32) -> Option<PdfPreview> {
        use cairo::{Context, Format, ImageSurface};
        use poppler::Document;

        // Load the PDF document
        let uri = format!("file://{}", path.display());
        let doc = Document::from_file(&uri, None).ok()?;

        let page_count = doc.n_pages() as usize;
        if page_count == 0 {
            return None;
        }

        // Get the first page
        let page = doc.page(0)?;
        let (page_width, page_height) = page.size();

        // Calculate scale to fit within max dimensions
        let scale_x = max_width as f64 / page_width;
        let scale_y = max_height as f64 / page_height;
        let scale = scale_x.min(scale_y).min(2.0); // Cap at 2x for quality

        let width = (page_width * scale) as i32;
        let height = (page_height * scale) as i32;

        // Create a Cairo surface to render to
        let mut surface = ImageSurface::create(Format::ARgb32, width, height).ok()?;

        {
            let ctx = Context::new(&surface).ok()?;

            // Fill with white background
            ctx.set_source_rgb(1.0, 1.0, 1.0);
            ctx.paint().ok()?;

            // Scale and render the page
            ctx.scale(scale, scale);
            page.render(&ctx);
        }

        // Get the pixel data
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().ok()?;

        // Convert from ARGB (Cairo) to RGBA
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = y * stride + x * 4;
                let b = data[offset];
                let g = data[offset + 1];
                let r = data[offset + 2];
                let a = data[offset + 3];
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }
        }

        Some(PdfPreview {
            data: rgba,
            width: width as u32,
            height: height as u32,
            page_count,
        })
    }

    /// Request loading a PDF preview. Returns the request ID.
    pub fn load(&mut self, path: PathBuf, max_width: u32, max_height: u32) -> u64 {
        self.next_request_id += 1;
        self.current_request_id = self.next_request_id;

        let request = PdfPreviewRequest {
            path,
            max_width,
            max_height,
            request_id: self.current_request_id,
        };

        let _ = self.request_tx.send(request);
        self.current_request_id
    }

    /// Poll for a completed preview load. Returns None if no result ready.
    pub fn poll(&mut self) -> Option<PdfPreviewResult> {
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

impl Default for PdfPreviewLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a file extension is a PDF.
pub fn is_pdf(extension: Option<&str>) -> bool {
    matches!(extension.map(|e| e.to_lowercase()).as_deref(), Some("pdf"))
}
