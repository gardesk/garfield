//! garfield-portal - XDG Desktop Portal backend for garfield file picker.
//!
//! This daemon implements the org.freedesktop.impl.portal.FileChooser interface,
//! spawning garfield in picker mode when applications request file dialogs.

mod file_chooser;
mod request;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};
use zbus::connection::Builder;

/// garfield-portal - XDG Desktop Portal backend
#[derive(Parser, Debug)]
#[command(name = "garfield-portal", about = "XDG Desktop Portal backend for garfield")]
struct Args {
    /// Run in foreground (don't daemonize)
    #[arg(long, short = 'f')]
    foreground: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let _args = Args::parse();

    tracing::info!("Starting garfield-portal");

    // Create the FileChooser interface
    let file_chooser = file_chooser::FileChooser::new();

    // Connect to session bus and serve the interface
    let _connection = Builder::session()?
        .name("org.freedesktop.impl.portal.desktop.garfield")?
        .serve_at("/org/freedesktop/portal/desktop", file_chooser)?
        .build()
        .await?;

    tracing::info!("garfield-portal listening on D-Bus session bus");

    // Keep the service running
    loop {
        // The connection handles incoming method calls automatically
        // We just need to keep the main task alive
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}
