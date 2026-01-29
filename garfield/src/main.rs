//! garfield - gar file explorer.

mod app;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::{fmt, EnvFilter};

/// garfield - gar file explorer
#[derive(Parser, Debug)]
#[command(name = "garfield", about = "gar file explorer", version)]
pub struct Args {
    /// Starting directory
    #[arg(value_name = "PATH")]
    pub start_dir: Option<PathBuf>,

    /// Enable file picker mode
    #[arg(long, short = 'p')]
    pub picker: bool,

    /// Select directories only (not files), requires --picker
    #[arg(long, short = 'd', requires = "picker")]
    pub directory: bool,

    /// Allow multiple selection, requires --picker
    #[arg(long, short = 'm', requires = "picker")]
    pub multiple: bool,

    /// File filter patterns (semicolon-separated, e.g., "*.rs;*.toml")
    #[arg(long, short = 'f', requires = "picker")]
    pub filter: Option<String>,

    /// Dialog title, requires --picker
    #[arg(long, short = 't', requires = "picker")]
    pub title: Option<String>,

    /// Custom accept button text (default: "Open")
    #[arg(long, requires = "picker")]
    pub accept_label: Option<String>,
}

/// Picker mode configuration parsed from CLI args.
#[derive(Debug, Clone)]
pub enum PickerMode {
    /// Normal file browser mode.
    None,
    /// Open file picker.
    OpenFile {
        multiple: bool,
        filters: Vec<String>,
    },
    /// Open directory picker.
    OpenDirectory {
        multiple: bool,
    },
}

impl PickerMode {
    /// Whether this is a picker mode (not normal browser).
    pub fn is_picker(&self) -> bool {
        !matches!(self, PickerMode::None)
    }

    /// Whether multiple selection is allowed.
    pub fn allows_multiple(&self) -> bool {
        match self {
            PickerMode::None => true,
            PickerMode::OpenFile { multiple, .. } => *multiple,
            PickerMode::OpenDirectory { multiple } => *multiple,
        }
    }

    /// Get file filters if any.
    pub fn filters(&self) -> &[String] {
        match self {
            PickerMode::OpenFile { filters, .. } => filters,
            _ => &[],
        }
    }

    /// Whether we're picking directories only.
    pub fn is_directory_mode(&self) -> bool {
        matches!(self, PickerMode::OpenDirectory { .. })
    }
}

/// Picker configuration derived from Args.
#[derive(Debug, Clone)]
pub struct PickerConfig {
    /// The picker mode.
    pub mode: PickerMode,
    /// Dialog title.
    pub title: Option<String>,
    /// Accept button label.
    pub accept_label: String,
}

impl PickerConfig {
    /// Create from command line arguments.
    pub fn from_args(args: &Args) -> Self {
        let mode = if args.picker {
            if args.directory {
                PickerMode::OpenDirectory {
                    multiple: args.multiple,
                }
            } else {
                let filters = args.filter
                    .as_ref()
                    .map(|f| f.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                    .unwrap_or_default();
                PickerMode::OpenFile {
                    multiple: args.multiple,
                    filters,
                }
            }
        } else {
            PickerMode::None
        };

        Self {
            mode,
            title: args.title.clone(),
            accept_label: args.accept_label.clone().unwrap_or_else(|| "Open".to_string()),
        }
    }

    /// Whether this is picker mode.
    pub fn is_picker(&self) -> bool {
        self.mode.is_picker()
    }
}

fn main() -> Result<()> {
    // Parse command line arguments first (before logging setup)
    let args = Args::parse();
    let picker_config = PickerConfig::from_args(&args);

    // Initialize logging - use stderr in picker mode to keep stdout clean for results
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    if picker_config.is_picker() {
        // Picker mode: log to stderr so stdout is clean for path output
        fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
        tracing::info!("Starting garfield in picker mode");
    } else {
        // Normal mode: log to stdout
        fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
        tracing::info!("Starting garfield");
    }

    // Create and run app
    let mut app = app::App::new(args.start_dir, picker_config)?;
    app.run()?;

    tracing::info!("garfield exiting");
    Ok(())
}
