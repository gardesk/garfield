//! File operations (copy, move, delete, rename).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Result of a file operation.
#[derive(Debug)]
pub struct OperationResult {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Files that were processed.
    pub processed: Vec<PathBuf>,
    /// Files that failed.
    pub failed: Vec<(PathBuf, String)>,
}

impl OperationResult {
    /// Create a successful result.
    pub fn success(processed: Vec<PathBuf>) -> Self {
        Self {
            success: true,
            error: None,
            processed,
            failed: Vec::new(),
        }
    }

    /// Create a failed result.
    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            error: Some(error),
            processed: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// Create a partial result.
    pub fn partial(processed: Vec<PathBuf>, failed: Vec<(PathBuf, String)>) -> Self {
        Self {
            success: failed.is_empty(),
            error: if failed.is_empty() {
                None
            } else {
                Some(format!("{} files failed", failed.len()))
            },
            processed,
            failed,
        }
    }
}

/// Copy a file or directory to a destination.
pub fn copy_path(source: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let file_name = source.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid source path")
    })?;
    let dest = dest_dir.join(file_name);

    if source.is_dir() {
        copy_dir_recursive(source, &dest)?;
    } else {
        fs::copy(source, &dest)?;
    }

    Ok(dest)
}

/// Copy a directory recursively.
fn copy_dir_recursive(source: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path)?;
        }
    }

    Ok(())
}

/// Move a file or directory to a destination.
pub fn move_path(source: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let file_name = source.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid source path")
    })?;
    let dest = dest_dir.join(file_name);

    // Try rename first (fast, same filesystem)
    if fs::rename(source, &dest).is_ok() {
        return Ok(dest);
    }

    // Fall back to copy + delete (cross-filesystem)
    if source.is_dir() {
        copy_dir_recursive(source, &dest)?;
        fs::remove_dir_all(source)?;
    } else {
        fs::copy(source, &dest)?;
        fs::remove_file(source)?;
    }

    Ok(dest)
}

/// Delete a file or directory permanently.
pub fn delete_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Rename a file or directory.
pub fn rename_path(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Cannot get parent directory")
    })?;
    let new_path = parent.join(new_name);

    if new_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("'{}' already exists", new_name),
        ));
    }

    fs::rename(path, &new_path)?;
    Ok(new_path)
}

/// Create a new directory.
pub fn create_directory(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let new_path = parent.join(name);

    if new_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("'{}' already exists", name),
        ));
    }

    fs::create_dir(&new_path)?;
    Ok(new_path)
}

/// Generate a unique name if the target already exists.
pub fn make_unique_name(dest_dir: &Path, name: &str) -> String {
    let path = dest_dir.join(name);
    if !path.exists() {
        return name.to_string();
    }

    // Split name into base and extension
    let (base, ext) = if let Some(dot_pos) = name.rfind('.') {
        (&name[..dot_pos], Some(&name[dot_pos..]))
    } else {
        (name, None)
    };

    // Try adding numbers until we find a unique name
    for i in 1..1000 {
        let new_name = match ext {
            Some(ext) => format!("{} ({}){}", base, i, ext),
            None => format!("{} ({})", base, i),
        };
        if !dest_dir.join(&new_name).exists() {
            return new_name;
        }
    }

    // Fallback with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match ext {
        Some(ext) => format!("{}_{}{}", base, timestamp, ext),
        None => format!("{}_{}", base, timestamp),
    }
}

/// Copy multiple files to a destination.
pub fn copy_files(sources: &[PathBuf], dest_dir: &Path) -> OperationResult {
    let mut processed = Vec::new();
    let mut failed = Vec::new();

    for source in sources {
        match copy_path(source, dest_dir) {
            Ok(dest) => processed.push(dest),
            Err(e) => failed.push((source.clone(), e.to_string())),
        }
    }

    OperationResult::partial(processed, failed)
}

/// Move multiple files to a destination.
pub fn move_files(sources: &[PathBuf], dest_dir: &Path) -> OperationResult {
    let mut processed = Vec::new();
    let mut failed = Vec::new();

    for source in sources {
        match move_path(source, dest_dir) {
            Ok(dest) => processed.push(dest),
            Err(e) => failed.push((source.clone(), e.to_string())),
        }
    }

    OperationResult::partial(processed, failed)
}

/// Delete multiple files permanently.
pub fn delete_files(paths: &[PathBuf]) -> OperationResult {
    let mut processed = Vec::new();
    let mut failed = Vec::new();

    for path in paths {
        match delete_path(path) {
            Ok(()) => processed.push(path.clone()),
            Err(e) => failed.push((path.clone(), e.to_string())),
        }
    }

    OperationResult::partial(processed, failed)
}
