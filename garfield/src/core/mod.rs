//! Core file system operations.

pub mod clipboard;
pub mod entry;
pub mod history;
pub mod operations;
pub mod trash;
pub mod undo;

pub use clipboard::{Clipboard, ClipboardOperation};
pub use entry::{
    read_directory, sort_entries, EntryType, FileEntry, SortDirection, SortOrder,
};
pub use history::History;
pub use operations::{
    copy_files, copy_path, copy_to_path, create_directory, delete_files, delete_path,
    make_unique_name, move_files, move_path, rename_path, OperationResult,
};
pub use trash::{empty_trash, restore_from_trash, trash_file, trash_files, trash_dir};
pub use undo::{FileOperation, UndoStack};
