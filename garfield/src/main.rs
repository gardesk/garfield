//! garfield - gar file explorer.

mod app;

use anyhow::Result;
use std::path::PathBuf;
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> Result<()> {
    // Initialize logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    tracing::info!("Starting garfield");

    // Parse command line arguments (simple for now)
    let start_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from);

    // Create and run app
    let mut app = app::App::new(start_dir)?;
    app.run()?;

    tracing::info!("garfield exiting");
    Ok(())
}
