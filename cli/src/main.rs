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
use directories::ProjectDirs;
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

  // Determine templates directory path using the updated logic
  let templates_path = determine_templates_dir(cli.templates_dir)?;
  log::info!("Using templates directory: {}", templates_path.display());
  if !templates_path.exists() {
    log::warn!("Selected templates directory '{}' does not exist. 'list' and 'generate' commands may find no templates.", templates_path.display());
    // Optionally create it? For now, just warn.
    // fs::create_dir_all(&templates_path).map_err(SpawnError::Io)?;
  }

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

//// Determines the templates directory path using a prioritized search.
///
/// Order of preference:
/// 1. --templates-dir CLI argument
/// 2. SPAWNPOINT_TEMPLATES_DIR environment variable
/// 3. User config directory (e.g., ~/.config/spawnpoint/templates)
/// 4. templates/ subdirectory relative to the executable
/// Fails if none are found and valid.
fn determine_templates_dir(cli_path_opt: Option<PathBuf>) -> Result<PathBuf, SpawnError> {
  // 1. Explicit CLI path
  if let Some(path) = cli_path_opt {
    log::debug!("Checking CLI option --templates-dir: {}", path.display());
    if path.is_dir() {
      log::trace!("Using CLI option --templates-dir path.");
      return Ok(path);
    } else {
      // Log a warning but continue searching other locations
      log::warn!(
        "Provided --templates-dir path is not a valid directory: {}",
        path.display()
      );
    }
  }

  // 2. Environment variable (Handled automatically by clap's `env` attribute if cli_path_opt was None,
  //    but we re-check here explicitly in case the CLI path was provided but invalid)
  if let Ok(env_path_str) = env::var("SPAWNPOINT_TEMPLATES_DIR") {
    let path = PathBuf::from(env_path_str);
    log::debug!(
      "Checking env var SPAWNPOINT_TEMPLATES_DIR: {}",
      path.display()
    );
    if path.is_dir() {
      log::trace!("Using env var SPAWNPOINT_TEMPLATES_DIR path.");
      return Ok(path);
    } else {
      log::warn!(
        "SPAWNPOINT_TEMPLATES_DIR path is not a valid directory: {}",
        path.display()
      );
    }
  }

  // 3. User config directory
  // Choose unique qualifiers for your app. Using GitHub username is common.
  if let Some(proj_dirs) = ProjectDirs::from("com", "excsn", "spawnpoint") {
    // Adjust "github_normano" if needed
    let config_dir = proj_dirs.config_dir();
    let path = config_dir.join("templates");
    log::debug!("Checking user config dir: {}", path.display());
    if path.is_dir() {
      log::trace!("Using user config directory path.");
      return Ok(path);
    } else {
      log::trace!("User config templates directory not found or not a directory.");
    }
  } else {
    log::warn!("Could not determine standard user config directory path.");
  }

  // 4. Relative to executable
  if let Ok(mut exe_path) = env::current_exe() {
    exe_path.pop(); // Remove the executable name itself
    let path = exe_path.join("templates");
    log::debug!("Checking executable relative dir: {}", path.display());
    if path.is_dir() {
      log::trace!("Using executable relative directory path.");
      return Ok(path);
    } else {
      log::trace!("Executable relative templates directory not found or not a directory.");
    }
  } else {
    log::warn!("Could not determine executable path.");
  }

  // 5. CWD relative (Removed - generally unreliable for installed tools)
  let cwd_path = PathBuf::from("templates");
  log::debug!("Checking CWD relative dir: {}", cwd_path.display());
  if cwd_path.is_dir() {
    return Ok(cwd_path);
  }

  // If we reach here, no valid directory was found
  log::error!("Could not find a valid templates directory. Searched CLI arg, env var, user config ({}), and executable relative paths.",
        ProjectDirs::from("com", "github_normano", "spawnpoint")
            .map(|p| p.config_dir().join("templates").display().to_string())
            .unwrap_or_else(|| "<user config path unavailable>".to_string())
    );
  Err(SpawnError::CannotDetermineTemplatesDir)
}
