//! Request tracking for portal dialogs.
//!
//! Each portal request gets a unique handle path and can be cancelled.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::Mutex;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};
use zbus::{interface, fdo};

/// A pending file chooser request.
pub struct PendingRequest {
    /// The child process running garfield.
    pub child: Child,
    /// Whether the request has been cancelled.
    pub cancelled: bool,
}

/// Manages pending requests.
#[derive(Clone, Default)]
pub struct RequestManager {
    requests: Arc<Mutex<HashMap<OwnedObjectPath, PendingRequest>>>,
}

impl RequestManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new pending request.
    pub async fn add(&self, handle: OwnedObjectPath, child: Child) {
        let mut requests = self.requests.lock().await;
        requests.insert(handle, PendingRequest {
            child,
            cancelled: false,
        });
    }

    /// Remove a request and return it.
    pub async fn remove(&self, handle: &ObjectPath<'_>) -> Option<PendingRequest> {
        let mut requests = self.requests.lock().await;
        requests.remove(&handle.to_owned())
    }

    /// Cancel a request by killing the child process.
    pub async fn cancel(&self, handle: &ObjectPath<'_>) -> bool {
        let mut requests = self.requests.lock().await;
        if let Some(request) = requests.get_mut(&handle.to_owned()) {
            request.cancelled = true;
            let _ = request.child.kill().await;
            true
        } else {
            false
        }
    }

    /// Check if a request was cancelled.
    #[allow(dead_code)]
    pub async fn is_cancelled(&self, handle: &ObjectPath<'_>) -> bool {
        let requests = self.requests.lock().await;
        requests.get(&handle.to_owned())
            .map(|r| r.cancelled)
            .unwrap_or(false)
    }
}

/// Request object that can be exported on D-Bus for cancellation.
pub struct Request {
    handle: OwnedObjectPath,
    manager: RequestManager,
}

impl Request {
    pub fn new(handle: OwnedObjectPath, manager: RequestManager) -> Self {
        Self { handle, manager }
    }
}

#[interface(name = "org.freedesktop.impl.portal.Request")]
impl Request {
    /// Close/cancel the request.
    async fn close(&self) -> fdo::Result<()> {
        // Log with backtrace info
        tracing::warn!("Close() called on request: {}", self.handle);
        let cancelled = self.manager.cancel(&self.handle.as_ref()).await;
        tracing::warn!("Cancel result: {} (true = child was killed)", cancelled);
        Ok(())
    }
}

/// Response codes for portal dialogs.
#[repr(u32)]
pub enum ResponseCode {
    /// User accepted the dialog.
    Success = 0,
    /// User cancelled the dialog.
    Cancelled = 1,
    /// An error occurred.
    Error = 2,
}

/// Build response results for file chooser.
pub fn build_file_chooser_response(
    paths: Vec<String>,
) -> HashMap<String, Value<'static>> {
    let mut results = HashMap::new();

    // Convert paths to file:// URIs
    let uris: Vec<Value> = paths
        .into_iter()
        .map(|p| Value::from(format!("file://{}", p)))
        .collect();

    results.insert("uris".to_string(), Value::Array(uris.into()));
    results
}
