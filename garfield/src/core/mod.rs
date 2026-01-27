//! Core file system operations.

pub mod clipboard;
pub mod entry;
pub mod history;
pub mod image_preview;
pub mod operations;
pub mod thumbnail;
pub mod preview_loader;
pub mod trash;
pub mod undo;

pub use clipboard::{Clipboard, ClipboardOperation};
pub use entry::{
    read_directory, sort_entries, EntryType, FileEntry, SortDirection, SortOrder,
};
pub use history::History;
pub use image_preview::{is_supported_image, ImagePreview, ImagePreviewLoader, ImagePreviewResult};
pub use operations::{
    copy_files, copy_path, copy_to_path, create_directory, delete_files, delete_path,
    make_unique_name, move_files, move_path, rename_path, OperationResult,
};
pub use preview_loader::{PreviewLoader, PreviewResult};
pub use thumbnail::{Thumbnail, ThumbnailLoader, THUMBNAIL_SIZE};
pub use trash::{empty_trash, restore_from_trash, trash_file, trash_files, trash_dir};
pub use undo::{FileOperation, UndoStack};
