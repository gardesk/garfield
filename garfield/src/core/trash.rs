//! Freedesktop trash support.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Get the trash directory path.
pub fn trash_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Trash"))
}

/// Get the trash files directory.
pub fn trash_files_dir() -> Option<PathBuf> {
    trash_dir().map(|d| d.join("files"))
}

/// Get the trash info directory.
pub fn trash_info_dir() -> Option<PathBuf> {
    trash_dir().map(|d| d.join("info"))
}

/// Ensure trash directories exist.
fn ensure_trash_dirs() -> io::Result<(PathBuf, PathBuf)> {
    let files_dir = trash_files_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Cannot determine trash directory")
    })?;
    let info_dir = trash_info_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Cannot determine trash info directory")
    })?;

    fs::create_dir_all(&files_dir)?;
    fs::create_dir_all(&info_dir)?;

    Ok((files_dir, info_dir))
}

/// Generate a unique trash name.
fn unique_trash_name(files_dir: &Path, name: &str) -> String {
    if !files_dir.join(name).exists() {
        return name.to_string();
    }

    let (base, ext) = if let Some(dot_pos) = name.rfind('.') {
        (&name[..dot_pos], Some(&name[dot_pos..]))
    } else {
        (name, None)
    };

    for i in 1..10000 {
        let new_name = match ext {
            Some(ext) => format!("{}.{}{}", base, i, ext),
            None => format!("{}.{}", base, i),
        };
        if !files_dir.join(&new_name).exists() {
            return new_name;
        }
    }

    // Fallback with timestamp
    let timestamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{}.{}", name, timestamp)
}

/// Create a .trashinfo file content.
fn create_trash_info(original_path: &Path) -> String {
    let path_encoded = original_path
        .to_string_lossy()
        .replace('%', "%25")
        .replace('\n', "%0A")
        .replace('\r', "%0D");

    let deletion_date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");

    format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        path_encoded, deletion_date
    )
}

/// Move a file to trash.
pub fn trash_file(path: &Path) -> io::Result<PathBuf> {
    let (files_dir, info_dir) = ensure_trash_dirs()?;

    let original_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid path"))?
        .to_string_lossy()
        .to_string();

    let trash_name = unique_trash_name(&files_dir, &original_name);
    let trash_path = files_dir.join(&trash_name);
    let info_path = info_dir.join(format!("{}.trashinfo", trash_name));

    // Write .trashinfo file first
    let info_content = create_trash_info(path);
    fs::write(&info_path, info_content)?;

    // Move file to trash
    if let Err(_) = fs::rename(path, &trash_path) {
        // If rename fails, try copy + delete (cross-filesystem)
        if path.is_dir() {
            copy_dir_recursive(path, &trash_path)?;
            fs::remove_dir_all(path)?;
        } else {
            fs::copy(path, &trash_path)?;
            fs::remove_file(path)?;
        }
    }

    Ok(trash_path)
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

/// Move multiple files to trash.
pub fn trash_files(paths: &[PathBuf]) -> Vec<Result<PathBuf, (PathBuf, String)>> {
    paths
        .iter()
        .map(|path| {
            trash_file(path)
                .map_err(|e| (path.clone(), e.to_string()))
        })
        .collect()
}

/// Restore a file from trash to its original location.
pub fn restore_from_trash(trash_name: &str) -> io::Result<PathBuf> {
    let (files_dir, info_dir) = ensure_trash_dirs()?;

    let trash_path = files_dir.join(trash_name);
    let info_path = info_dir.join(format!("{}.trashinfo", trash_name));

    if !trash_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "File not found in trash",
        ));
    }

    // Read original path from .trashinfo
    let info_content = fs::read_to_string(&info_path)?;
    let original_path = parse_trash_info_path(&info_content).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Invalid .trashinfo file")
    })?;

    // Restore the file
    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::rename(&trash_path, &original_path)?;
    fs::remove_file(&info_path)?;

    Ok(original_path)
}

/// Parse the original path from .trashinfo content.
fn parse_trash_info_path(content: &str) -> Option<PathBuf> {
    for line in content.lines() {
        if let Some(path) = line.strip_prefix("Path=") {
            // URL decode the path
            let decoded = path
                .replace("%0A", "\n")
                .replace("%0D", "\r")
                .replace("%25", "%");
            return Some(PathBuf::from(decoded));
        }
    }
    None
}

/// Empty the trash.
pub fn empty_trash() -> io::Result<()> {
    let (files_dir, info_dir) = ensure_trash_dirs()?;

    // Remove all files
    if files_dir.exists() {
        for entry in fs::read_dir(&files_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
    }

    // Remove all .trashinfo files
    if info_dir.exists() {
        for entry in fs::read_dir(&info_dir)? {
            let entry = entry?;
            fs::remove_file(entry.path())?;
        }
    }

    Ok(())
}
