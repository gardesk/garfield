//! XDG Recently Used files manager.
//!
//! Parses and manages `~/.local/share/recently-used.xbel` (XBEL format).
//! This provides system-wide recently accessed files from any application.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};
use thiserror::Error;

/// Maximum number of recent entries to track.
const MAX_ENTRIES: usize = 25;

/// Errors from recents operations.
#[derive(Debug, Error)]
pub enum RecentsError {
    #[error("Failed to read xbel file: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Invalid timestamp format")]
    InvalidTimestamp,
}

/// A recently accessed file from the XDG recently-used.xbel file.
#[derive(Debug, Clone)]
pub struct RecentEntry {
    /// File path (decoded from file:// URI).
    pub path: PathBuf,
    /// MIME type (e.g., "image/png", "inode/directory").
    pub mime_type: Option<String>,
    /// Last visit timestamp.
    pub visited: SystemTime,
    /// Last modification timestamp.
    pub modified: SystemTime,
}

impl RecentEntry {
    /// Returns true if this entry is a directory.
    pub fn is_directory(&self) -> bool {
        self.mime_type
            .as_ref()
            .is_some_and(|m| m == "inode/directory")
    }

    /// Returns the file name.
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_name().and_then(|n| n.to_str())
    }

    /// Returns the parent directory.
    pub fn parent_dir(&self) -> Option<&Path> {
        self.path.parent()
    }
}

/// Manager for XDG recently-used files.
pub struct RecentsManager {
    /// Path to recently-used.xbel file.
    xbel_path: PathBuf,
    /// Cached entries (sorted by visited time, most recent first).
    entries: Vec<RecentEntry>,
    /// Maximum entries to display.
    max_entries: usize,
}

impl RecentsManager {
    /// Create a new RecentsManager with default path.
    pub fn new() -> Self {
        let xbel_path = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("recently-used.xbel");

        Self {
            xbel_path,
            entries: Vec::new(),
            max_entries: MAX_ENTRIES,
        }
    }

    /// Load/refresh entries from ~/.local/share/recently-used.xbel
    pub fn load(&mut self) -> Result<(), RecentsError> {
        self.entries.clear();

        if !self.xbel_path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&self.xbel_path)?;
        let reader = BufReader::new(file);
        let mut xml = Reader::from_reader(reader);
        xml.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut current_bookmark: Option<PartialBookmark> = None;
        let mut in_metadata = false;

        loop {
            match xml.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    match e.name().as_ref() {
                        b"bookmark" => {
                            current_bookmark = Some(parse_bookmark_attrs(e));
                        }
                        b"mime:mime-type" if current_bookmark.is_some() && in_metadata => {
                            if let Some(ref mut bm) = current_bookmark {
                                bm.mime_type = get_attr(e, b"type");
                            }
                        }
                        b"metadata" => {
                            in_metadata = true;
                        }
                        _ => {}
                    }

                    // Handle empty elements (self-closing tags)
                    if matches!(xml.read_event_into(&mut buf), Ok(Event::End(_))) {
                        // This was actually a start tag, not empty
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"bookmark" => {
                        if let Some(bm) = current_bookmark.take() {
                            if let Some(entry) = bm.into_entry() {
                                // Only include file:// entries that still exist
                                if entry.path.exists() {
                                    self.entries.push(entry);
                                }
                            }
                        }
                    }
                    b"metadata" => {
                        in_metadata = false;
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(RecentsError::Xml(e)),
                _ => {}
            }
            buf.clear();
        }

        // Sort by visited time, most recent first
        self.entries
            .sort_by(|a, b| b.visited.cmp(&a.visited));

        // Limit to max entries
        self.entries.truncate(self.max_entries);

        Ok(())
    }

    /// Get entries (files that still exist, limited to max_entries).
    pub fn entries(&self) -> &[RecentEntry] {
        &self.entries
    }

    /// Check if there are any entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Add an entry when garfield opens a file (writes to xbel).
    /// This makes garfield a good citizen of the XDG ecosystem.
    pub fn add_entry(&mut self, path: &Path, mime_type: &str) -> Result<(), RecentsError> {
        // Read existing bookmarks
        let mut bookmarks = self.read_all_bookmarks()?;

        // Create file URI
        let uri = format!("file://{}", percent_encode(&path.to_string_lossy()));
        let now = Utc::now();
        let now_str = now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

        // Check if entry already exists
        if let Some(existing) = bookmarks.iter_mut().find(|b| b.href == uri) {
            // Update existing entry
            existing.visited = now_str.clone();
            existing.modified = now_str;
            existing.app_count += 1;
        } else {
            // Add new entry
            bookmarks.push(StoredBookmark {
                href: uri,
                added: now_str.clone(),
                modified: now_str.clone(),
                visited: now_str,
                mime_type: mime_type.to_string(),
                app_count: 1,
            });
        }

        // Write back to file
        self.write_bookmarks(&bookmarks)?;

        // Reload cache
        self.load()
    }

    /// Read all bookmarks from the xbel file (for rewriting).
    fn read_all_bookmarks(&self) -> Result<Vec<StoredBookmark>, RecentsError> {
        let mut bookmarks = Vec::new();

        if !self.xbel_path.exists() {
            return Ok(bookmarks);
        }

        let file = fs::File::open(&self.xbel_path)?;
        let reader = BufReader::new(file);
        let mut xml = Reader::from_reader(reader);
        xml.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut current: Option<StoredBookmark> = None;
        let mut in_metadata = false;

        loop {
            match xml.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    match e.name().as_ref() {
                        b"bookmark" => {
                            let href = get_attr(e, b"href").unwrap_or_default();
                            let added = get_attr(e, b"added").unwrap_or_default();
                            let modified = get_attr(e, b"modified").unwrap_or_default();
                            let visited = get_attr(e, b"visited").unwrap_or_default();
                            current = Some(StoredBookmark {
                                href,
                                added,
                                modified,
                                visited,
                                mime_type: String::new(),
                                app_count: 1,
                            });
                        }
                        b"mime:mime-type" if current.is_some() && in_metadata => {
                            if let Some(ref mut bm) = current {
                                bm.mime_type = get_attr(e, b"type").unwrap_or_default();
                            }
                        }
                        b"bookmark:application" if current.is_some() => {
                            if let Some(ref mut bm) = current {
                                if let Some(count_str) = get_attr(e, b"count") {
                                    bm.app_count = count_str.parse().unwrap_or(1);
                                }
                            }
                        }
                        b"metadata" => {
                            in_metadata = true;
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"bookmark" => {
                        if let Some(bm) = current.take() {
                            // Keep all bookmarks, not just file:// ones
                            bookmarks.push(bm);
                        }
                    }
                    b"metadata" => {
                        in_metadata = false;
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => return Err(RecentsError::Xml(e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(bookmarks)
    }

    /// Write bookmarks back to the xbel file.
    fn write_bookmarks(&self, bookmarks: &[StoredBookmark]) -> Result<(), RecentsError> {
        let file = File::create(&self.xbel_path)?;
        let mut writer = Writer::new_with_indent(BufWriter::new(file), b' ', 2);

        // XML declaration
        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

        // Root element with namespaces
        let mut xbel = BytesStart::new("xbel");
        xbel.push_attribute(("version", "1.0"));
        xbel.push_attribute(("xmlns:bookmark", "http://www.freedesktop.org/standards/desktop-bookmarks"));
        xbel.push_attribute(("xmlns:mime", "http://www.freedesktop.org/standards/shared-mime-info"));
        writer.write_event(Event::Start(xbel))?;

        // Write each bookmark
        for bookmark in bookmarks {
            self.write_bookmark(&mut writer, bookmark)?;
        }

        // Close root
        writer.write_event(Event::End(BytesEnd::new("xbel")))?;

        writer.into_inner().flush()?;
        Ok(())
    }

    /// Write a single bookmark element.
    fn write_bookmark<W: Write>(&self, writer: &mut Writer<W>, bookmark: &StoredBookmark) -> Result<(), RecentsError> {
        let mut elem = BytesStart::new("bookmark");
        elem.push_attribute(("href", bookmark.href.as_str()));
        elem.push_attribute(("added", bookmark.added.as_str()));
        elem.push_attribute(("modified", bookmark.modified.as_str()));
        elem.push_attribute(("visited", bookmark.visited.as_str()));
        writer.write_event(Event::Start(elem))?;

        // <info>
        writer.write_event(Event::Start(BytesStart::new("info")))?;

        // <metadata>
        let mut metadata = BytesStart::new("metadata");
        metadata.push_attribute(("owner", "http://freedesktop.org"));
        writer.write_event(Event::Start(metadata))?;

        // <mime:mime-type>
        if !bookmark.mime_type.is_empty() {
            let mut mime = BytesStart::new("mime:mime-type");
            mime.push_attribute(("type", bookmark.mime_type.as_str()));
            writer.write_event(Event::Empty(mime))?;
        }

        // <bookmark:applications>
        writer.write_event(Event::Start(BytesStart::new("bookmark:applications")))?;

        // <bookmark:application>
        let mut app = BytesStart::new("bookmark:application");
        app.push_attribute(("name", "garfield"));
        app.push_attribute(("exec", "garfield %u"));
        app.push_attribute(("modified", bookmark.modified.as_str()));
        app.push_attribute(("count", bookmark.app_count.to_string().as_str()));
        writer.write_event(Event::Empty(app))?;

        writer.write_event(Event::End(BytesEnd::new("bookmark:applications")))?;
        writer.write_event(Event::End(BytesEnd::new("metadata")))?;
        writer.write_event(Event::End(BytesEnd::new("info")))?;
        writer.write_event(Event::End(BytesEnd::new("bookmark")))?;

        Ok(())
    }

    /// Clear all cached entries (does not modify the file).
    pub fn clear_cache(&mut self) {
        self.entries.clear();
    }
}

impl Default for RecentsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// A bookmark as stored in the xbel file (for rewriting).
struct StoredBookmark {
    href: String,
    added: String,
    modified: String,
    visited: String,
    mime_type: String,
    app_count: u32,
}

/// Partial bookmark being parsed.
struct PartialBookmark {
    href: Option<String>,
    visited: Option<String>,
    modified: Option<String>,
    mime_type: Option<String>,
}

impl PartialBookmark {
    fn into_entry(self) -> Option<RecentEntry> {
        let href = self.href?;

        // Only handle file:// URIs
        if !href.starts_with("file://") {
            return None;
        }

        // Decode file:// URI to path
        let path_str = &href[7..]; // Strip "file://"
        let decoded = percent_decode(path_str);
        let path = PathBuf::from(decoded);

        let visited = parse_iso8601(&self.visited.unwrap_or_default())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let modified = parse_iso8601(&self.modified.unwrap_or_default())
            .unwrap_or(SystemTime::UNIX_EPOCH);

        Some(RecentEntry {
            path,
            mime_type: self.mime_type,
            visited,
            modified,
        })
    }
}

/// Parse bookmark element attributes.
fn parse_bookmark_attrs(e: &BytesStart) -> PartialBookmark {
    PartialBookmark {
        href: get_attr(e, b"href"),
        visited: get_attr(e, b"visited"),
        modified: get_attr(e, b"modified"),
        mime_type: None,
    }
}

/// Get an attribute value as String.
fn get_attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
}

/// Percent-encode a path for use in file:// URIs.
fn percent_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            // Safe characters (unreserved in RFC 3986 + / for paths)
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' => {
                result.push(c);
            }
            // Everything else gets percent-encoded
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

/// Percent-decode a URL path component.
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Try to parse the next two characters as hex
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            // If parsing failed, keep the original
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }

    result
}

/// Parse ISO 8601 timestamp to SystemTime.
fn parse_iso8601(s: &str) -> Option<SystemTime> {
    // Format: 2026-01-12T11:08:52.375556Z
    let dt: DateTime<Utc> = s.parse().ok()?;
    Some(SystemTime::from(dt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("file%3A%2F%2F"), "file://");
        assert_eq!(percent_decode("no_encoding"), "no_encoding");
    }

    #[test]
    fn test_parse_iso8601() {
        let ts = parse_iso8601("2026-01-12T11:08:52.375556Z");
        assert!(ts.is_some());
    }

    #[test]
    fn test_recents_manager_new() {
        let manager = RecentsManager::new();
        assert!(manager.entries.is_empty());
        assert!(manager.xbel_path.ends_with("recently-used.xbel"));
    }
}
