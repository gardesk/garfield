//! File entry type with metadata.

use std::cmp::Ordering;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Type of file system entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Other (device, socket, etc.).
    Other,
}

impl EntryType {
    /// Determine entry type from metadata.
    pub fn from_metadata(meta: &Metadata) -> Self {
        if meta.is_dir() {
            EntryType::Directory
        } else if meta.is_file() {
            EntryType::File
        } else if meta.is_symlink() {
            EntryType::Symlink
        } else {
            EntryType::Other
        }
    }
}

/// A file system entry with metadata.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// File name (not full path).
    pub name: String,
    /// Full path.
    pub path: PathBuf,
    /// Entry type.
    pub entry_type: EntryType,
    /// File size in bytes (0 for directories).
    pub size: u64,
    /// Last modified time.
    pub modified: Option<SystemTime>,
    /// Whether this is a hidden file (starts with '.').
    pub hidden: bool,
    /// Whether this is a symlink.
    pub is_symlink: bool,
    /// Symlink target (if symlink).
    pub symlink_target: Option<PathBuf>,
}

impl FileEntry {
    /// Create a FileEntry from a path.
    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        // Get symlink metadata first (doesn't follow symlinks)
        let symlink_meta = fs::symlink_metadata(path)?;
        let is_symlink = symlink_meta.is_symlink();

        // For symlinks, also get the target metadata
        let (meta, symlink_target) = if is_symlink {
            let target = fs::read_link(path).ok();
            // Try to get target metadata, fall back to symlink metadata
            let target_meta = fs::metadata(path).unwrap_or(symlink_meta.clone());
            (target_meta, target)
        } else {
            (symlink_meta, None)
        };

        let entry_type = if is_symlink {
            EntryType::Symlink
        } else {
            EntryType::from_metadata(&meta)
        };

        let size = if meta.is_file() { meta.len() } else { 0 };
        let modified = meta.modified().ok();
        let hidden = name.starts_with('.');

        Ok(Self {
            name,
            path: path.to_path_buf(),
            entry_type,
            size,
            modified,
            hidden,
            is_symlink,
            symlink_target,
        })
    }

    /// Check if this is a directory.
    pub fn is_dir(&self) -> bool {
        self.entry_type == EntryType::Directory
    }

    /// Check if this is a file.
    pub fn is_file(&self) -> bool {
        self.entry_type == EntryType::File
    }

    /// Check if this entry can be navigated into (directory or symlink to directory).
    pub fn is_navigable(&self) -> bool {
        if self.entry_type == EntryType::Directory {
            return true;
        }
        if self.is_symlink {
            // Check if symlink points to a directory
            self.path.is_dir()
        } else {
            false
        }
    }

    /// Get file extension (lowercase).
    pub fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
    }

    /// Format size for display.
    pub fn format_size(&self) -> String {
        if self.is_dir() {
            return String::new();
        }
        format_bytes(self.size)
    }

    /// Format modified time for display.
    pub fn format_modified(&self) -> String {
        match self.modified {
            Some(time) => {
                let datetime: chrono::DateTime<chrono::Local> = time.into();
                datetime.format("%Y-%m-%d %H:%M").to_string()
            }
            None => String::new(),
        }
    }
}

/// Sort order for file entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

/// Read directory entries.
pub fn read_directory(path: impl AsRef<Path>) -> std::io::Result<Vec<FileEntry>> {
    let path = path.as_ref();
    let mut entries = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if let Ok(file_entry) = FileEntry::from_path(entry.path()) {
            entries.push(file_entry);
        }
    }

    Ok(entries)
}

/// Sort entries with directories first.
pub fn sort_entries(entries: &mut [FileEntry], order: SortOrder, direction: SortDirection) {
    entries.sort_by(|a, b| {
        // Directories always come first
        match (a.is_dir(), b.is_dir()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }

        let cmp = match order {
            SortOrder::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortOrder::Size => a.size.cmp(&b.size),
            SortOrder::Modified => a.modified.cmp(&b.modified),
            SortOrder::Type => a.extension().cmp(&b.extension()),
        };

        match direction {
            SortDirection::Ascending => cmp,
            SortDirection::Descending => cmp.reverse(),
        }
    });
}

/// Check if a filename matches a glob pattern.
///
/// Supports simple patterns:
/// - `*.ext` - matches any file ending with `.ext`
/// - `name.*` - matches any file starting with `name.`
/// - `exact` - matches exact filename
pub fn matches_filter(filename: &str, pattern: &str) -> bool {
    let filename_lower = filename.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if pattern_lower == "*" {
        return true;
    }

    if let Some(suffix) = pattern_lower.strip_prefix("*.") {
        // *.ext pattern
        filename_lower.ends_with(&format!(".{}", suffix))
    } else if let Some(prefix) = pattern_lower.strip_suffix(".*") {
        // name.* pattern
        filename_lower.starts_with(&format!("{}.", prefix))
    } else if pattern_lower.contains('*') {
        // More complex patterns - split by * and check if parts exist in order
        let parts: Vec<&str> = pattern_lower.split('*').collect();
        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if i == 0 && !filename_lower.starts_with(part) {
                return false;
            }
            if i == parts.len() - 1 && !filename_lower.ends_with(part) {
                return false;
            }
            if let Some(found_pos) = filename_lower[pos..].find(part) {
                pos += found_pos + part.len();
            } else {
                return false;
            }
        }
        true
    } else {
        // Exact match
        filename_lower == pattern_lower
    }
}

/// Check if a file entry matches any of the given filter patterns.
/// Directories always match (for navigation).
/// Empty filters match everything.
pub fn matches_any_filter(entry: &FileEntry, filters: &[String]) -> bool {
    // Directories always visible for navigation
    if entry.is_dir() {
        return true;
    }

    // No filters means show everything
    if filters.is_empty() {
        return true;
    }

    // Check each filter
    filters.iter().any(|pattern| matches_filter(&entry.name, pattern))
}

/// Format bytes as human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
