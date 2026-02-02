//! FileChooser portal interface implementation.
//!
//! Implements org.freedesktop.impl.portal.FileChooser by spawning
//! garfield in picker mode.

use crate::request::{build_file_chooser_response, Request, RequestManager, ResponseCode};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
use zbus::{fdo, interface};

/// FileChooser portal backend.
pub struct FileChooser {
    request_manager: RequestManager,
}

impl FileChooser {
    pub fn new() -> Self {
        Self {
            request_manager: RequestManager::new(),
        }
    }

    /// Parse X11 window ID from portal parent_window string.
    /// Format is "x11:<xid>" where xid is the window ID in hex.
    fn parse_parent_window(parent_window: &str) -> Option<u32> {
        if parent_window.starts_with("x11:") {
            let hex_str = &parent_window[4..];
            u32::from_str_radix(hex_str, 16).ok()
        } else {
            None
        }
    }

    /// Spawn garfield in picker mode and collect results.
    async fn spawn_picker(
        &self,
        handle: OwnedObjectPath,
        title: &str,
        directory_mode: bool,
        multiple: bool,
        filters: Vec<String>,
        current_folder: Option<String>,
        parent_window: Option<u32>,
    ) -> (u32, HashMap<String, Value<'static>>) {
        // Find garfield binary - prefer ~/.cargo/bin (development), then /usr/local/bin, then PATH
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo_path = format!("{}/.cargo/bin/garfield", home);
        let garfield_path = if std::path::Path::new(&cargo_path).exists() {
            cargo_path
        } else if std::path::Path::new("/usr/local/bin/garfield").exists() {
            "/usr/local/bin/garfield".to_string()
        } else {
            "garfield".to_string() // Fall back to PATH lookup
        };

        let mut cmd = Command::new(garfield_path);
        cmd.arg("--picker");

        if !title.is_empty() {
            cmd.arg("--title").arg(title);
        }

        if directory_mode {
            cmd.arg("--directory");
        }

        if multiple {
            cmd.arg("--multiple");
        }

        if !filters.is_empty() {
            cmd.arg("--filter").arg(filters.join(";"));
        }

        if let Some(parent) = parent_window {
            cmd.arg("--parent-window").arg(parent.to_string());
        }

        if let Some(folder) = current_folder {
            cmd.arg(&folder);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit()); // Let garfield stderr through for debugging

        tracing::info!("Spawning garfield picker: {:?}", cmd);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to spawn garfield: {}", e);
                return (ResponseCode::Error as u32, HashMap::new());
            }
        };

        tracing::info!("garfield spawned with PID {:?}", child.id());

        // Get stdout handle before adding to manager (we need ownership)
        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                tracing::error!("Failed to get stdout from garfield");
                return (ResponseCode::Error as u32, HashMap::new());
            }
        };

        // Track the request for cancellation (child still runs, just stdout detached)
        self.request_manager.add(handle.clone(), child).await;

        // Wait for the child to exit by reading stdout until EOF
        tracing::info!("Waiting for garfield to complete...");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut paths = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            if !line.is_empty() {
                tracing::debug!("garfield output: {}", line);
                paths.push(line);
            }
        }

        tracing::info!("garfield stdout closed, got {} paths", paths.len());
        for (i, path) in paths.iter().enumerate() {
            tracing::info!("  path[{}]: {:?}", i, path);
        }

        // Now remove from manager and check status
        let request = self.request_manager.remove(&handle.as_ref()).await;

        // Check if cancelled
        if let Some(req) = &request {
            if req.cancelled {
                tracing::info!("Request was cancelled");
                return (ResponseCode::Cancelled as u32, HashMap::new());
            }
        }

        // If we got paths, it's a success
        if paths.is_empty() {
            tracing::info!("No paths selected, treating as cancelled");
            (ResponseCode::Cancelled as u32, HashMap::new())
        } else {
            tracing::info!("Returning {} selected paths", paths.len());
            let response = build_file_chooser_response(paths);
            tracing::info!("Response: {:?}", response);
            (ResponseCode::Success as u32, response)
        }
    }

    /// Parse filter options from the portal format.
    fn parse_filters(options: &HashMap<&str, Value<'_>>) -> Vec<String> {
        // Portal filters are: a(sa(us)) - array of (name, array of (type, pattern))
        // For now, we'll extract glob patterns
        let mut result = Vec::new();

        if let Some(Value::Array(filters_array)) = options.get("filters") {
            for filter in filters_array.iter() {
                // Each filter is (name, patterns_array)
                if let Value::Structure(s) = filter {
                    let fields = s.fields();
                    if fields.len() >= 2 {
                        if let Value::Array(patterns) = &fields[1] {
                            for pattern in patterns.iter() {
                                // Each pattern is (type, pattern_string)
                                // type 0 = glob, type 1 = mime
                                if let Value::Structure(ps) = pattern {
                                    let pfields = ps.fields();
                                    if pfields.len() >= 2 {
                                        if let (Value::U32(0), Value::Str(glob)) = (&pfields[0], &pfields[1]) {
                                            result.push(glob.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        result
    }

    /// Extract current folder from options.
    fn parse_current_folder(options: &HashMap<&str, Value<'_>>) -> Option<String> {
        if let Some(Value::Array(bytes)) = options.get("current_folder") {
            // current_folder is a byte array (path as bytes)
            let path_bytes: Vec<u8> = bytes.iter()
                .filter_map(|v| {
                    if let Value::U8(b) = v {
                        Some(*b)
                    } else {
                        None
                    }
                })
                .collect();

            // Remove trailing null if present
            let path_bytes: Vec<u8> = path_bytes.into_iter()
                .take_while(|&b| b != 0)
                .collect();

            String::from_utf8(path_bytes).ok()
        } else {
            None
        }
    }

    /// Extract current_name (suggested filename) from options for SaveFile.
    fn parse_current_name(options: &HashMap<&str, Value<'_>>) -> Option<String> {
        if let Some(Value::Str(s)) = options.get("current_name") {
            Some(s.to_string())
        } else {
            None
        }
    }

    /// Spawn garfield in save mode.
    async fn spawn_save_picker(
        &self,
        handle: OwnedObjectPath,
        title: &str,
        suggested_filename: Option<String>,
        current_folder: Option<String>,
        parent_window: Option<u32>,
    ) -> (u32, HashMap<String, Value<'static>>) {
        // Find garfield binary - prefer ~/.cargo/bin (development), then /usr/local/bin, then PATH
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo_path = format!("{}/.cargo/bin/garfield", home);
        let garfield_path = if std::path::Path::new(&cargo_path).exists() {
            cargo_path
        } else if std::path::Path::new("/usr/local/bin/garfield").exists() {
            "/usr/local/bin/garfield".to_string()
        } else {
            "garfield".to_string()
        };

        let mut cmd = Command::new(&garfield_path);
        cmd.arg("--picker");
        cmd.arg("--save");

        if let Some(filename) = suggested_filename {
            cmd.arg("--save-filename").arg(&filename);
        }

        if !title.is_empty() {
            cmd.arg("--title").arg(title);
        }

        if let Some(parent) = parent_window {
            cmd.arg("--parent-window").arg(parent.to_string());
        }

        if let Some(folder) = current_folder {
            cmd.arg(&folder);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        tracing::info!("Spawning garfield save picker: {:?}", cmd);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to spawn garfield: {}", e);
                return (ResponseCode::Error as u32, HashMap::new());
            }
        };

        tracing::info!("garfield save picker spawned with PID {:?}", child.id());

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                tracing::error!("Failed to get stdout from garfield");
                return (ResponseCode::Error as u32, HashMap::new());
            }
        };

        self.request_manager.add(handle.clone(), child).await;

        tracing::info!("Waiting for garfield save picker to complete...");
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut paths = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            if !line.is_empty() {
                tracing::debug!("garfield output: {}", line);
                paths.push(line);
            }
        }

        tracing::info!("garfield save picker completed, got {} paths", paths.len());
        for (i, path) in paths.iter().enumerate() {
            tracing::info!("  path[{}]: {:?}", i, path);
        }

        let request = self.request_manager.remove(&handle.as_ref()).await;

        if let Some(req) = &request {
            if req.cancelled {
                tracing::info!("Request was cancelled");
                return (ResponseCode::Cancelled as u32, HashMap::new());
            }
        }

        if paths.is_empty() {
            tracing::info!("No path selected, treating as cancelled");
            (ResponseCode::Cancelled as u32, HashMap::new())
        } else {
            tracing::info!("Returning save path: {:?}", paths[0]);
            let response = build_file_chooser_response(paths);
            tracing::info!("Response: {:?}", response);
            (ResponseCode::Success as u32, response)
        }
    }
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    /// Open a file chooser dialog.
    ///
    /// Portal method for opening files.
    async fn open_file(
        &self,
        #[zbus(object_server)] server: &zbus::ObjectServer,
        handle: ObjectPath<'_>,
        _app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        tracing::info!("OpenFile request: handle={}, title={}, parent_window={}", handle, title, parent_window);

        let handle_owned: OwnedObjectPath = handle.into();
        let parent_window_id = Self::parse_parent_window(parent_window);

        // Parse options
        let multiple = options.get("multiple")
            .and_then(|v| if let Value::Bool(b) = v { Some(*b) } else { None })
            .unwrap_or(false);

        let directory = options.get("directory")
            .and_then(|v| if let Value::Bool(b) = v { Some(*b) } else { None })
            .unwrap_or(false);

        let filters = Self::parse_filters(&options);
        let current_folder = Self::parse_current_folder(&options);

        tracing::debug!("Registering request object at {}", handle_owned);

        // Register request object for cancellation
        let request = Request::new(handle_owned.clone(), self.request_manager.clone());
        server.at(handle_owned.as_ref(), request).await
            .map_err(|e| fdo::Error::Failed(format!("Failed to register request: {}", e)))?;

        tracing::debug!("Request object registered, spawning picker");

        // Spawn picker and wait for result
        let result = self.spawn_picker(
            handle_owned.clone(),
            title,
            directory,
            multiple,
            filters,
            current_folder,
            parent_window_id,
        ).await;

        tracing::debug!("Picker returned: {:?}", result.0);

        // Remove request object
        let _ = server.remove::<Request, _>(&handle_owned).await;

        tracing::info!("OpenFile returning response code {}", result.0);
        Ok(result)
    }

    /// Save a file dialog.
    ///
    /// Portal method for saving files.
    async fn save_file(
        &self,
        #[zbus(object_server)] server: &zbus::ObjectServer,
        handle: ObjectPath<'_>,
        _app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        tracing::info!("SaveFile request: handle={}, title={}, parent_window={}", handle, title, parent_window);
        tracing::debug!("SaveFile options: {:?}", options);

        let handle_owned: OwnedObjectPath = handle.into();
        let parent_window_id = Self::parse_parent_window(parent_window);
        let current_folder = Self::parse_current_folder(&options);
        let suggested_filename = Self::parse_current_name(&options);

        tracing::info!("SaveFile: folder={:?}, filename={:?}, parent={:?}", current_folder, suggested_filename, parent_window_id);

        let request = Request::new(handle_owned.clone(), self.request_manager.clone());
        server.at(handle_owned.as_ref(), request).await
            .map_err(|e| fdo::Error::Failed(format!("Failed to register request: {}", e)))?;

        // Spawn picker in save mode with suggested filename
        let result = self.spawn_save_picker(
            handle_owned.clone(),
            title,
            suggested_filename,
            current_folder,
            parent_window_id,
        ).await;

        let _ = server.remove::<Request, _>(&handle_owned).await;

        Ok(result)
    }

    /// Save multiple files.
    async fn save_files(
        &self,
        #[zbus(object_server)] server: &zbus::ObjectServer,
        handle: ObjectPath<'_>,
        _app_id: &str,
        parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        tracing::info!("SaveFiles request: handle={}, title={}, parent_window={}", handle, title, parent_window);

        // SaveFiles picks a directory for saving multiple files
        let handle_owned: OwnedObjectPath = handle.into();
        let parent_window_id = Self::parse_parent_window(parent_window);
        let current_folder = Self::parse_current_folder(&options);

        let request = Request::new(handle_owned.clone(), self.request_manager.clone());
        server.at(handle_owned.as_ref(), request).await
            .map_err(|e| fdo::Error::Failed(format!("Failed to register request: {}", e)))?;

        let result = self.spawn_picker(
            handle_owned.clone(),
            title,
            true,
            false,
            Vec::new(),
            current_folder,
            parent_window_id,
        ).await;

        let _ = server.remove::<Request, _>(&handle_owned).await;

        Ok(result)
    }
}
