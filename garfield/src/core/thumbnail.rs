//! Thumbnail loading and caching for grid view.
//!
//! Provides async thumbnail generation with in-memory caching.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Default thumbnail size.
pub const THUMBNAIL_SIZE: u32 = 64;

/// Maximum number of cached thumbnails.
const MAX_CACHE_SIZE: usize = 500;

/// A loaded thumbnail.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    /// RGBA pixel data.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Request to load a thumbnail.
#[derive(Debug, Clone)]
struct ThumbnailRequest {
    path: PathBuf,
    size: u32,
    request_id: u64,
}

/// Result of a thumbnail load.
#[derive(Debug)]
pub struct ThumbnailResult {
    pub path: PathBuf,
    pub thumbnail: Option<Thumbnail>,
}

/// Manages async thumbnail loading with caching.
pub struct ThumbnailLoader {
    /// Sender for requests.
    request_tx: Sender<ThumbnailRequest>,
    /// Receiver for results.
    result_rx: Receiver<ThumbnailResult>,
    /// Worker thread.
    _worker: JoinHandle<()>,
    /// Request ID counter.
    next_request_id: u64,
    /// Paths currently being loaded.
    pending: HashMap<PathBuf, u64>,
    /// In-memory cache.
    cache: HashMap<PathBuf, Thumbnail>,
    /// LRU order tracking (most recently used at end).
    lru_order: Vec<PathBuf>,
}

impl ThumbnailLoader {
    /// Create a new thumbnail loader.
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<ThumbnailRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ThumbnailResult>();

        let worker = thread::spawn(move || {
            Self::worker_loop(request_rx, result_tx);
        });

        Self {
            request_tx,
            result_rx,
            _worker: worker,
            next_request_id: 0,
            pending: HashMap::new(),
            cache: HashMap::new(),
            lru_order: Vec::new(),
        }
    }

    /// Worker thread loop.
    fn worker_loop(request_rx: Receiver<ThumbnailRequest>, result_tx: Sender<ThumbnailResult>) {
        while let Ok(request) = request_rx.recv() {
            let thumbnail = Self::load_thumbnail(&request.path, request.size);
            let _ = result_tx.send(ThumbnailResult {
                path: request.path,
                thumbnail,
            });
        }
    }

    /// Load and scale a thumbnail.
    fn load_thumbnail(path: &PathBuf, size: u32) -> Option<Thumbnail> {
        // Check if it's a PDF
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        if ext.as_deref() == Some("pdf") {
            return Self::load_pdf_thumbnail(path, size);
        }

        Self::load_image_thumbnail(path, size)
    }

    /// Load thumbnail from an image file.
    fn load_image_thumbnail(path: &PathBuf, size: u32) -> Option<Thumbnail> {
        use image::GenericImageView;

        let img = image::open(path).ok()?;
        let (orig_w, orig_h) = img.dimensions();

        // Calculate scale to fit within size while preserving aspect ratio
        let scale = (size as f64 / orig_w as f64).min(size as f64 / orig_h as f64).min(1.0);
        let new_w = (orig_w as f64 * scale) as u32;
        let new_h = (orig_h as f64 * scale) as u32;

        // Use fast nearest-neighbor for thumbnails
        let resized = if scale < 1.0 {
            img.resize(new_w, new_h, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        let rgba = resized.to_rgba8();
        let (width, height) = rgba.dimensions();

        Some(Thumbnail {
            data: rgba.into_raw(),
            width,
            height,
        })
    }

    /// Load thumbnail from a PDF file (render first page).
    fn load_pdf_thumbnail(path: &PathBuf, size: u32) -> Option<Thumbnail> {
        use cairo::{Context, Format, ImageSurface};
        use poppler::Document;

        // Load the PDF document
        let uri = format!("file://{}", path.display());
        let doc = Document::from_file(&uri, None).ok()?;

        if doc.n_pages() == 0 {
            return None;
        }

        // Get the first page
        let page = doc.page(0)?;
        let (page_width, page_height) = page.size();

        // Calculate scale to fit within thumbnail size
        let scale = (size as f64 / page_width).min(size as f64 / page_height);
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

        Some(Thumbnail {
            data: rgba,
            width: width as u32,
            height: height as u32,
        })
    }

    /// Get a cached thumbnail, or request loading if not cached.
    /// Returns Some(thumbnail) if cached, None if loading or not an image.
    pub fn get_or_load(&mut self, path: &PathBuf) -> Option<&Thumbnail> {
        // Check cache first
        if self.cache.contains_key(path) {
            // Update LRU order
            self.touch_lru(path);
            return self.cache.get(path);
        }

        // Check if already pending
        if self.pending.contains_key(path) {
            return None;
        }

        // Request loading
        self.next_request_id += 1;
        let request_id = self.next_request_id;

        let request = ThumbnailRequest {
            path: path.clone(),
            size: THUMBNAIL_SIZE,
            request_id,
        };

        if self.request_tx.send(request).is_ok() {
            self.pending.insert(path.clone(), request_id);
        }

        None
    }

    /// Poll for completed thumbnail loads.
    pub fn poll(&mut self) -> Vec<PathBuf> {
        let mut loaded = Vec::new();

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    self.pending.remove(&result.path);
                    if let Some(thumbnail) = result.thumbnail {
                        self.insert_cache(result.path.clone(), thumbnail);
                        loaded.push(result.path);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        loaded
    }

    /// Insert into cache with LRU eviction.
    fn insert_cache(&mut self, path: PathBuf, thumbnail: Thumbnail) {
        // Evict if at capacity
        while self.cache.len() >= MAX_CACHE_SIZE {
            if let Some(oldest) = self.lru_order.first().cloned() {
                self.cache.remove(&oldest);
                self.lru_order.remove(0);
            } else {
                break;
            }
        }

        self.cache.insert(path.clone(), thumbnail);
        self.lru_order.push(path);
    }

    /// Update LRU order for a path.
    fn touch_lru(&mut self, path: &PathBuf) {
        if let Some(pos) = self.lru_order.iter().position(|p| p == path) {
            self.lru_order.remove(pos);
            self.lru_order.push(path.clone());
        }
    }

    /// Clear the cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.lru_order.clear();
    }

    /// Check if a path is cached.
    pub fn is_cached(&self, path: &PathBuf) -> bool {
        self.cache.contains_key(path)
    }

    /// Get a cached thumbnail without requesting a load.
    pub fn get_cached(&self, path: &PathBuf) -> Option<&Thumbnail> {
        self.cache.get(path)
    }
}

impl Default for ThumbnailLoader {
    fn default() -> Self {
        Self::new()
    }
}
