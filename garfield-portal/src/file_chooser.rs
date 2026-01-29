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

    /// Spawn garfield in picker mode and collect results.
    async fn spawn_picker(
        &self,
        handle: OwnedObjectPath,
        title: &str,
        directory_mode: bool,
        multiple: bool,
        filters: Vec<String>,
        current_folder: Option<String>,
    ) -> (u32, HashMap<String, Value<'static>>) {
        // Use full path to ensure we get the right garfield binary
        // (user may have old version in ~/.cargo/bin before /usr/local/bin in PATH)
        let garfield_path = if std::path::Path::new("/usr/local/bin/garfield").exists() {
            "/usr/local/bin/garfield"
        } else {
            "garfield" // Fall back to PATH lookup
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
        _parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        tracing::info!("OpenFile request: handle={}, title={}", handle, title);

        let handle_owned: OwnedObjectPath = handle.into();

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
        _parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        tracing::info!("SaveFile request: handle={}, title={}", handle, title);

        // For now, save dialogs work like open dialogs but for directories
        // A full implementation would show a save dialog with filename input
        let handle_owned: OwnedObjectPath = handle.into();
        let current_folder = Self::parse_current_folder(&options);

        let request = Request::new(handle_owned.clone(), self.request_manager.clone());
        server.at(handle_owned.as_ref(), request).await
            .map_err(|e| fdo::Error::Failed(format!("Failed to register request: {}", e)))?;

        // For save, we pick a directory and the caller handles the filename
        let result = self.spawn_picker(
            handle_owned.clone(),
            title,
            true, // directory mode for save location
            false,
            Vec::new(),
            current_folder,
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
        _parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> fdo::Result<(u32, HashMap<String, Value<'static>>)> {
        tracing::info!("SaveFiles request: handle={}, title={}", handle, title);

        // SaveFiles picks a directory for saving multiple files
        let handle_owned: OwnedObjectPath = handle.into();
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
        ).await;

        let _ = server.remove::<Request, _>(&handle_owned).await;

        Ok(result)
    }
}
