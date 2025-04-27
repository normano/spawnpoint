// src/error.rs
use std::{path::PathBuf, process::ExitStatus};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SpawnError {
  #[error("IO Error: {0}")]
  Io(#[from] std::io::Error),

  #[error("YAML Parsing Error: {0}")]
  YamlParse(#[from] serde_yaml::Error),

  #[error("Template directory not found at path: {0}")]
  TemplateDirNotFound(PathBuf),

  #[error("Invalid template path (not a directory): {0}")]
  InvalidTemplatePath(PathBuf),

  #[error("Could not read scaffold manifest '{manifest_path}': {source}")]
  ManifestReadError {
    manifest_path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Could not parse scaffold manifest '{manifest_path}': {source}")]
  ManifestParseError {
    manifest_path: PathBuf,
    #[source]
    source: serde_yaml::Error,
  },

  #[error("Failed to create output directory '{path}': {source}")]
  OutputDirCreation {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Error during project generation: {0}")]
  GenerationError(String),

  #[error("Error walking template directory '{path}': {source}")]
  WalkDirError {
    path: PathBuf,
    #[source]
    source: walkdir::Error,
  },

  #[error("Validation failed on step '{step_name}': {reason}")]
  ValidationError { step_name: String, reason: String },

  #[error("Command Execution Error for step '{step_name}': {source}")]
  CommandExecError {
    step_name: String,
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>, // Box to handle different error types
  },
  #[error("Command for step '{step_name}' failed with status {status}. Stderr: {stderr}")]
  CommandFailedStatus {
    step_name: String,
    status: ExitStatus, // Store the actual status
    stdout: String,
    stderr: String,
  },
  #[error("Command for step '{step_name}' produced stderr (check_stderr=true). Stderr: {stderr}")]
  CommandStderrNotEmpty {
    step_name: String,
    stdout: String,
    stderr: String,
  },

  #[error("User interaction failed: {0}")]
  DialoguerError(#[from] dialoguer::Error),

  #[error("Could not determine templates directory")]
  CannotDetermineTemplatesDir,
}

// Helper to convert generic command errors
impl SpawnError {
  fn command_exec_error<E>(step_name: &str, error: E) -> Self
  where
    E: std::error::Error + Send + Sync + 'static,
  {
    SpawnError::CommandExecError {
      step_name: step_name.to_string(),
      source: Box::new(error),
    }
  }
}
