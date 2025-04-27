// src/main.rs
mod cli;
mod config;
mod error;
mod generate; // Stub
mod list;
mod utils;
mod validate; // Stub // Stub

use clap::Parser;
use cli::{Cli, Commands};
use error::SpawnError;
use log::LevelFilter;
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), SpawnError> {
  let cli = Cli::parse();

  // Setup logging based on verbosity
  let log_level = match cli.verbose {
    0 => LevelFilter::Info,
    1 => LevelFilter::Debug,
    _ => LevelFilter::Trace,
  };
  env_logger::Builder::new().filter_level(log_level).init();

  log::debug!("CLI args: {:?}", cli);

  // Determine templates directory path
  let templates_path = determine_templates_dir(cli.templates_dir)?;
  log::info!("Using templates directory: {}", templates_path.display());

  // Match on the command
  match cli.command {
    Commands::List => {
      list::run_list(&templates_path)?;
    }
    Commands::Generate(args) => {
      generate::run_generate(args, &templates_path)?;
    }
    Commands::Validate(args) => {
      validate::run_validate(args, &templates_path)?;
    }
  }

  Ok(())
}

/// Determines the templates directory path.
/// Order of preference:
/// 1. --templates-dir CLI argument
/// 2. SPAWNPOINT_TEMPLATES_DIR environment variable
/// 3. templates/ subdirectory relative to the executable
/// 4. templates/ subdirectory relative to the current working directory (fallback)
fn determine_templates_dir(cli_path: Option<PathBuf>) -> Result<PathBuf, SpawnError> {
  if let Some(path) = cli_path {
    if path.is_dir() {
      return Ok(path);
    } else {
      log::warn!(
        "Provided --templates-dir path does not exist or is not a directory: {}",
        path.display()
      );
    }
  }

  // Env variable check happens automatically via clap's `env` attribute

  // Relative to executable
  if let Ok(mut exe_path) = env::current_exe() {
    exe_path.pop(); // Remove the executable name
    let path = exe_path.join("templates");
    if path.is_dir() {
      return Ok(path);
    }
  }

  // Relative to current working directory as a last resort
  let path = PathBuf::from("templates");
  if path.is_dir() {
    return Ok(path);
  }

  Err(SpawnError::CannotDetermineTemplatesDir)
}
