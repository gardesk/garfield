//! garfieldctl - CLI control tool for garfield.

use anyhow::Result;
use clap::{Parser, Subcommand};
use garfield_ipc::{Command, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "garfieldctl")]
#[command(about = "Control garfield file manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open a directory in garfield.
    Open {
        /// Path to open.
        path: PathBuf,
        /// Open in new tab.
        #[arg(short = 't', long)]
        new_tab: bool,
    },
    /// Get the current directory.
    CurrentDir,
    /// Get garfield status.
    Status,
    /// Quit garfield.
    Quit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cmd = match cli.command {
        Commands::Open { path, new_tab } => Command::Open {
            path: path.to_string_lossy().to_string(),
            new_tab,
        },
        Commands::CurrentDir => Command::CurrentDir,
        Commands::Status => Command::Status,
        Commands::Quit => Command::Quit,
    };

    let response = send_command(&cmd)?;

    if response.success {
        if let Some(data) = response.data {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    } else {
        eprintln!("Error: {}", response.error.unwrap_or_default());
        std::process::exit(1);
    }

    Ok(())
}

fn send_command(cmd: &Command) -> Result<Response> {
    let socket_path = garfield_ipc::socket_path();
    let mut stream = UnixStream::connect(&socket_path)?;

    // Send command as JSON line
    let json = serde_json::to_string(cmd)?;
    writeln!(stream, "{}", json)?;
    stream.flush()?;

    // Read response
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let response: Response = serde_json::from_str(&line)?;
    Ok(response)
}
