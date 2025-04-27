// src/list.rs
use crate::config::ScaffoldManifest;
use crate::error::SpawnError;
use log::{debug, warn};
use std::fs;
use std::path::Path;

pub fn run_list(templates_dir: &Path) -> Result<(), SpawnError> {
  println!("Available Spawn Point Templates:");
  println!("{:<25} | {:<15} | {}", "Name", "Language", "Description");
  println!("{:-<25}-+-{:-<15}-+-{:-<50}", "", "", ""); // Separator

  if !templates_dir.is_dir() {
    warn!(
      "Templates directory not found or is not a directory: {}",
      templates_dir.display()
    );
    return Ok(()); // Or return an error? Let's allow running list even if empty/missing.
  }

  for entry_result in fs::read_dir(templates_dir)? {
    let entry = match entry_result {
      Ok(e) => e,
      Err(e) => {
        warn!("Failed to read entry in templates directory: {}", e);
        continue;
      }
    };

    let path = entry.path();
    if path.is_dir() {
      let manifest_path = path.join("scaffold.yaml");
      if manifest_path.is_file() {
        match read_and_parse_manifest(&manifest_path) {
          Ok(manifest) => {
            println!(
              "{:<25} | {:<15} | {}",
              manifest.name, manifest.language, manifest.description
            );
          }
          Err(e) => {
            warn!(
              "Skipping directory '{}': Could not read or parse scaffold.yaml: {}",
              path
                .file_name()
                .map_or_else(|| ".".into(), |n| n.to_string_lossy()),
              e
            );
          }
        }
      } else {
        debug!(
          "Directory {} does not contain scaffold.yaml, skipping.",
          path.display()
        );
      }
    }
  }

  Ok(())
}

pub(crate) fn read_and_parse_manifest(manifest_path: &Path) -> Result<ScaffoldManifest, SpawnError> {
  let content = fs::read_to_string(manifest_path).map_err(|e| SpawnError::ManifestReadError {
    manifest_path: manifest_path.to_path_buf(),
    source: e,
  })?;
  serde_yaml::from_str(&content).map_err(|e| SpawnError::ManifestParseError {
    manifest_path: manifest_path.to_path_buf(),
    source: e,
  })
}
