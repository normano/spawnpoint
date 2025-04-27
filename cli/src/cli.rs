// src/cli.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "spawnpoint", // Command name users type
    author,
    version,
    about = "Generates project scaffolds from predefined templates with validation.",
    long_about = None
)]
pub struct Cli {
  #[command(subcommand)]
  pub command: Commands,

  /// Increase verbosity level (e.g., -v, -vv)
  #[arg(short, long, action = clap::ArgAction::Count)]
  pub verbose: u8,
  
  #[arg(long)] // Configures the --templates-dir command-line flag
  #[clap(env = "SPAWNPOINT_TEMPLATES_DIR")] // Configures the environment variable fallback
  pub templates_dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
  /// List available templates
  List,
  /// Generate a new project scaffold
  Generate(GenerateArgs),
  /// Validate a specific template within the scaffolder
  Validate(ValidateArgs),
}

#[derive(Parser, Debug)]
pub struct GenerateArgs {
  /// Language/Framework of the template (e.g., nodejs, rust)
  #[arg(short, long)]
  pub language: Option<String>,

  /// Specific template name (e.g., nodejs_core_app_v1)
  #[arg(short, long)]
  pub template: Option<String>,

  /// Directory to generate the project into
  #[arg(short, long, default_value = ".")]
  pub output_dir: PathBuf,
  // TODO: Add non-interactive variable flags if needed:
  // #[arg(long)]
  // pub var: Vec<String>, // e.g., --var name=value
}

#[derive(Parser, Debug)]
pub struct ValidateArgs {
  /// Language/Framework of the template to validate
  pub language: String,

  /// Specific template name to validate
  pub template: String,
}
