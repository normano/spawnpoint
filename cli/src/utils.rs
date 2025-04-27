// src/utils.rs
use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;
use walkdir::WalkDir;

use crate::config::{
  CaseTransformation, Condition, PlaceholderFilenames, ScaffoldManifest, ValidationStep,
  VariableDefinition,
};
use crate::error::SpawnError;

/// Takes base variables and computes transformed versions based on manifest definitions.
/// The key in the returned map will be the *placeholder* string (e.g., "__PASCAL_VAR__").
/// The value will be the transformed user input.
pub fn compute_transformed_variables(
  base_variables: &HashMap<String, String>, // User input keyed by var name (e.g., "appName")
  variable_definitions: &[VariableDefinition], // From manifest
) -> HashMap<String, String> {
  let mut all_substitutions = HashMap::new();
  let mut computed_base_transforms: HashMap<String, HashMap<CaseTransformation, String>> =
    HashMap::new(); // Cache base transforms

  // --- Pass 1: Compute base transformations and store direct placeholders ---
  for var_def in variable_definitions {
    if let Some(base_value) = base_variables.get(&var_def.name) {
      // Insert direct value if it was prompted for
      if var_def.prompt.is_some() {
        all_substitutions.insert(var_def.placeholder_value.clone(), base_value.clone());
      }

      // Compute and cache transformations
      let mut transforms = HashMap::new();
      for (transform_case, transform_placeholder) in &var_def.transformations {
        let transformed_value = match transform_case {
          CaseTransformation::PascalCase => base_value.to_pascal_case(),
          CaseTransformation::CamelCase => base_value.to_lower_camel_case(),
          CaseTransformation::SnakeCase => base_value.to_snake_case(),
          CaseTransformation::KebabCase => base_value.to_kebab_case(),
          CaseTransformation::ShoutySnakeCase => base_value.to_shouty_snake_case(),
        };
        // Store computed value keyed by placeholder
        all_substitutions.insert(transform_placeholder.clone(), transformed_value.clone());
        // Also cache it keyed by CaseTransformation enum for later use
        transforms.insert(transform_case.clone(), transformed_value);
      }
      computed_base_transforms.insert(var_def.name.clone(), transforms);
    }
  }

  // --- Pass 2: Compute derived/combined values ---
  // Example: Specifically compute the full package name
  // Find the definition for the fullPackageName placeholder
  if let Some(full_name_def) = variable_definitions
    .iter()
    .find(|vd| vd.name == "fullPackageName")
  {
    // Find by specific name
    // Get required base variable values from user input map
    let use_scope = base_variables
      .get("useOrgScope")
      .map_or(false, |s| s == "true");
    let scope = base_variables.get("orgScope").cloned().unwrap_or_default(); // Default to empty if missing

    // Get the already computed kebab-case version of projectName
    let kebab_project_name = computed_base_transforms
      .get("projectName")
      .and_then(|transforms| transforms.get(&CaseTransformation::KebabCase))
      .cloned()
      .unwrap_or_else(|| {
          warn!("KebabCase transformation for 'projectName' not found/computed for 'fullPackageName'. Falling back.");
          // Fallback: compute it directly if needed (less efficient)
          base_variables.get("projectName").map_or_else(String::new, |pn| pn.to_kebab_case())
      });

    let final_name = if use_scope && !scope.is_empty() && !kebab_project_name.is_empty() {
      format!("{}/{}", scope, kebab_project_name)
    } else {
      kebab_project_name // Just the kebab name if no scope or scope is empty
    };

    // Insert the final computed name keyed by its placeholder
    all_substitutions.insert(full_name_def.placeholder_value.clone(), final_name);
  }
  // --- End Pass 2 ---

  all_substitutions
}

pub fn copy_template_dir(
  template_path: &Path,
  output_path: &Path,
  substitutions: &HashMap<String, String>,
  manifest: &ScaffoldManifest,
) -> Result<(), SpawnError> {
  debug!(
    "Copying template from {} to {}",
    template_path.display(),
    output_path.display()
  );

  // --- Pass 1: Count files respecting conditions ---
  let base_variables_for_condition = manifest
    .variables
    .iter()
    .filter_map(|vd| {
      substitutions
        .get(&vd.placeholder_value)
        .map(|val| (vd.name.clone(), val.clone()))
    })
    .collect::<HashMap<String, String>>();

  let mut file_count: u64 = 0;
  let mut count_walker = WalkDir::new(template_path).into_iter();
  loop {
    let entry_result = match count_walker.next() {
      Some(res) => res,
      None => break,
    };
    let entry = match entry_result {
      Ok(e) => e,
      Err(walk_err) => {
        warn!("Error accessing path during count: {}", walk_err);
        if let Some(path) = walk_err.path() {
          if path.is_dir() {
            count_walker.skip_current_dir();
          }
        }
        continue;
      }
    };
    if entry.path() == template_path {
      continue;
    }

    let relative_path = match entry.path().strip_prefix(template_path) {
      Ok(p) => p,
      Err(_) => continue,
    };
    let relative_path_str = relative_path.to_string_lossy().to_string();
    let mut skip_entry = false;
    if let Some(condition) = manifest.conditional_paths.get(&relative_path_str) {
      if !evaluate_condition(condition, &base_variables_for_condition) {
        skip_entry = true;
        if entry.file_type().is_dir() {
          count_walker.skip_current_dir();
        }
      }
    }

    if !skip_entry && entry.file_type().is_file() {
      // Skip manifest itself
      if entry
        .path()
        .file_name()
        .map_or(false, |name| name == "scaffold.yaml")
      {
        continue;
      }
      file_count += 1;
    }
  }
  debug!("Total files to process: {}", file_count);

  // --- Setup Progress Bar ---
  let pb = ProgressBar::new(file_count);
  pb.set_style(
    ProgressStyle::default_bar()
      .template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
      )
      .expect("Failed to set progress bar style") // Panic if template is invalid
      .progress_chars("#>-"),
  );
  pb.set_message("Copying files...");

  // --- Pass 2: Copy files with progress ---
  let mut walker = WalkDir::new(template_path).into_iter();
  loop {
    let entry_result = match walker.next() {
      Some(res) => res,
      None => break, // End of iteration
    };

    let entry = match entry_result {
      Ok(e) => e,
      Err(walk_err) => {
        // Log error accessing path but continue if possible
        warn!("Error accessing path during walk: {}", walk_err);
        if let Some(path) = walk_err.path() {
          // If the error was on a directory, try to skip it
          if path.is_dir() {
            walker.skip_current_dir();
          }
        }
        continue;
      }
    };

    let current_path = entry.path();

    // Skip the root template directory itself
    if current_path == template_path {
      continue;
    }

    let relative_path = match current_path.strip_prefix(template_path) {
      Ok(p) => p,
      Err(e) => {
        warn!(
          "Failed to strip prefix {} from {}: {}. Skipping.",
          template_path.display(),
          current_path.display(),
          e
        );
        continue;
      }
    };

    // --- Conditional Check ---
    // Convert relative_path to string for map lookup (lossy conversion is okay here)
    let relative_path_str = relative_path.to_string_lossy().to_string();
    let mut skip_entry = false;
    if let Some(condition) = manifest.conditional_paths.get(&relative_path_str) {
      trace!("Found condition for path: {}", relative_path_str);
      if !evaluate_condition(condition, &base_variables_for_condition) {
        info!("Condition not met for '{}', skipping.", relative_path_str);
        skip_entry = true;
        // If it's a directory, skip its contents too
        if entry.file_type().is_dir() {
          walker.skip_current_dir();
        }
      } else {
        trace!("Condition met for path: {}", relative_path_str);
      }
    }

    if skip_entry {
      continue; // Skip the rest of the loop for this entry
    }
    // --- End Conditional Check ---

    // --- Path Substitution Logic ---
    let mut substituted_relative_path = PathBuf::new();
    let placeholder_config = &manifest.placeholder_filenames;
    if manifest.placeholder_filenames.is_some() {
      for component in relative_path.components() {
        if let Some(segment_str) = component.as_os_str().to_str() {
          let substituted_segment = substitute_path_segment(
            segment_str,
            substitutions,
            placeholder_config,
            &manifest.variables,
          );
          substituted_relative_path.push(substituted_segment);
        } else {
          warn!("Non-UTF8 path component: {:?}", component);
          substituted_relative_path.push(component.as_os_str());
        }
      }
    } else {
      substituted_relative_path = relative_path.to_path_buf();
    }
    let output_entry_path = output_path.join(&substituted_relative_path);
    // --- End Path Substitution Logic ---

    if entry.file_type().is_dir() {
      // Use entry.file_type() instead of current_path.is_dir()
      trace!("Creating directory: {}", output_entry_path.display());
      fs::create_dir_all(&output_entry_path).map_err(|e| SpawnError::OutputDirCreation {
        path: output_entry_path.clone(),
        source: e,
      })?;
    } else if entry.file_type().is_file() {
      if current_path
        .file_name()
        .map_or(false, |name| name == "scaffold.yaml")
      {
        continue;
      }

      pb.set_message(format!("Processing {}", relative_path.display()));

      if let Some(parent) = output_entry_path.parent() {
        if !parent.exists() {
          trace!("Creating parent directory for file: {}", parent.display());
          fs::create_dir_all(parent)?;
        }
      }

      if is_binary(relative_path, manifest) {
        trace!("Copying binary file to: {}", output_entry_path.display());
        fs::copy(current_path, &output_entry_path)?;
      } else {
        trace!(
          "Reading and substituting text file: {}",
          current_path.display()
        );
        let content = fs::read_to_string(current_path)?;
        let substituted_content = substitute_content(&content, substitutions, manifest);
        trace!(
          "Writing substituted file to: {}",
          output_entry_path.display()
        );
        // Use write instead of write_all for potential large files?
        // For simplicity, fs::write is fine for typical template sizes.
        fs::write(&output_entry_path, substituted_content)?;
      }
      pb.inc(1);
    } else {
      log::debug!(
        "Skipping non-file/non-directory entry: {}",
        current_path.display()
      );
    }
  }

  pb.finish_with_message("File processing complete."); // Final message
  Ok(())
}

/// Evaluates a condition based on the provided base variables.
fn evaluate_condition(condition: &Condition, base_variables: &HashMap<String, String>) -> bool {
  match base_variables.get(&condition.variable) {
    Some(actual_value) => actual_value.eq_ignore_ascii_case(&condition.value),
    None => {
      warn!(
        "Conditional variable '{}' not found in provided variables.",
        condition.variable
      );
      false // Condition cannot be met if variable doesn't exist
    }
  }
}

/// Checks if a path (relative to the template root) should be treated as binary.
fn is_binary(relative_path: &Path, manifest: &ScaffoldManifest) -> bool {
  // Check by specific file path first
  if manifest
    .binary_files
    .iter()
    .any(|bin_file| bin_file == relative_path)
  {
    return true;
  }

  // Check by extension
  if let Some(ext) = relative_path.extension().and_then(|os| os.to_str()) {
    let ext_with_dot = format!(".{}", ext);
    if manifest
      .binary_extensions
      .iter()
      .any(|bin_ext| bin_ext == &ext_with_dot || bin_ext == ext)
    // Check with and without dot
    {
      return true;
    }
  }

  false
}

/// Performs simple string replacement based on manifest variables and placeholder values.
pub fn substitute_content(
  content: &str,
  substitutions: &HashMap<String, String>,
  _manifest: &ScaffoldManifest, // Keep for potential future use, but not needed now
) -> String {
  let mut current_content = content.to_string();
  // Iterate directly over the substitutions map
  for (placeholder, value) in substitutions {
    current_content = current_content.replace(placeholder, value);
  }
  current_content
}

/// Performs variable substitution on a single path segment (filename or directory name)
/// using the placeholder filename markers defined in the manifest.
fn substitute_path_segment(
  segment: &str,
  substitutions: &HashMap<String, String>,
  placeholder_config: &Option<PlaceholderFilenames>,
  variable_definitions: &[VariableDefinition],
) -> String {
  let Some(config) = placeholder_config else {
    return segment.to_string();
  };

  let mut current_segment = segment.to_string();

  // We need to know which placeholders correspond to filename variables
  for var_def in variable_definitions {
    // Construct the potential marker based on the variable name
    let marker = format!("{}{}{}", config.prefix, var_def.name, config.suffix);
    // Check if this marker exists in the segment
    if current_segment.contains(&marker) {
      // Find the corresponding *value* from the base variables map (user input)
      // This assumes the path segment uses the {{varName}} style marker, not the content placeholder!
      // Let's refine this: Path substitution should use the prefix/suffix style.
      if let Some(_value) = substitutions.get(&var_def.placeholder_value) {
        // Get the original value first
        // Now apply transformations *if needed* based on the structure of the marker?
        // This gets complex. Let's simplify: assume path markers map directly to original variable names.
        // The marker IS e.g. __VAR_myVar__
        if let Some(user_value) = substitutions.get(&var_def.placeholder_value) {
          // Still use original value
          current_segment = current_segment.replace(&marker, user_value);
        }
      } else {
        warn!(
          "Variable '{}' used in path marker '{}' but not found in substitutions map.",
          var_def.name, marker
        );
      }
    }
    // Also check for transformation placeholders in the path segment
    for (_case, transform_placeholder) in &var_def.transformations {
      // Construct the marker like __VAR_PascalCase_myVar__ ? Or just use the placeholder directly?
      // Let's assume paths only use the main variable marker for simplicity for now.
      // If transform_placeholder appears in the segment string, replace it.
      if current_segment.contains(transform_placeholder) {
        if let Some(transformed_value) = substitutions.get(transform_placeholder) {
          current_segment = current_segment.replace(transform_placeholder, transformed_value);
        } else {
          warn!("Transformation placeholder '{}' used in path segment but not found in substitutions map.", transform_placeholder);
        }
      }
    }
  }
  current_segment
}

/// Executes a validation step command.
pub fn run_command(
  step: &ValidationStep,
  working_dir: &Path, // The actual directory to run in
  base_variables: &HashMap<String, String>,
) -> Result<Output, SpawnError> {
  let substituted_command = substitute_command_for_validation(&step.command, base_variables);
  info!(
    "Executing step '{}': `{}` in {}",
    step.name,
    substituted_command,
    working_dir.display()
  );

  let mut cmd = Command::new("sh"); // Using sh -c for simplicity
  cmd.arg("-c").arg(&substituted_command);

  cmd.current_dir(working_dir);
  cmd.envs(&step.env);

  cmd.stdout(Stdio::piped());
  cmd.stderr(Stdio::piped());

  let mut child = cmd.spawn().map_err(|e| SpawnError::CommandExecError {
    step_name: step.name.clone(),
    source: Box::new(e),
  })?;

  // --- Basic Timeout Handling (more robust solutions exist) ---
  let timeout = step.timeout_secs.map(Duration::from_secs);
  let status = match timeout {
    Some(duration) => {
      // This is a simplified timeout check. It doesn't guarantee killing grandchildren.
      // Crates like `wait-timeout` handle this better.
      match child.try_wait() {
        Ok(Some(status)) => Ok(status), // Exited quickly
        Ok(None) => {
          // Not exited yet, wait with timeout
          thread::sleep(duration);
          match child.try_wait() {
            Ok(Some(status)) => Ok(status), // Exited within timeout
            Ok(None) => {
              // Timeout exceeded, kill the process
              warn!(
                "Step '{}' timed out after {}s. Attempting to kill.",
                step.name,
                duration.as_secs()
              );
              child.kill().map_err(|e| SpawnError::CommandExecError {
                step_name: step.name.clone(),
                source: Box::new(e),
              })?;
              Err(SpawnError::CommandExecError {
                step_name: step.name.clone(),
                source: format!("Step timed out after {} seconds", duration.as_secs()).into(), // Create a boxed error
              })
            }
            Err(e) => Err(SpawnError::CommandExecError {
              step_name: step.name.clone(),
              source: Box::new(e),
            }),
          }
        }
        Err(e) => Err(SpawnError::CommandExecError {
          step_name: step.name.clone(),
          source: Box::new(e),
        }),
      }
    }
    None => child.wait().map_err(|e| SpawnError::CommandExecError {
      step_name: step.name.clone(),
      source: Box::new(e),
    }),
  };

  // Capture output even if status indicated timeout/kill error
  let mut stdout_str = String::new();
  let mut stderr_str = String::new();
  // Use take() to get ownership of the handles, allowing reading after wait/kill
  if let Some(mut stdout_handle) = child.stdout.take() {
    stdout_handle
      .read_to_string(&mut stdout_str)
      .unwrap_or_else(|e| {
        warn!("Failed to read stdout for step '{}': {}", step.name, e);
        0 // .unwrap_or_else returns the value inside closure
      });
  }
  if let Some(mut stderr_handle) = child.stderr.take() {
    stderr_handle
      .read_to_string(&mut stderr_str)
      .unwrap_or_else(|e| {
        warn!("Failed to read stderr for step '{}': {}", step.name, e);
        0
      });
  }

  debug!("Step '{}' stdout:\n{}", step.name, stdout_str);
  debug!("Step '{}' stderr:\n{}", step.name, stderr_str);

  // Now handle the status determined earlier
  match status {
    Ok(exit_status) => {
      let output = Output {
        status: exit_status,
        stdout: stdout_str.into_bytes(),
        stderr: stderr_str.into_bytes(),
      };

      // Check exit status
      if !output.status.success() {
        warn!(
          "Step '{}' exited with non-zero status: {:?}",
          step.name,
          output.status.code()
        );
        // Don't return Err here if ignore_errors is true, let the caller decide
        // But still return the output object
        return Ok(output); // Caller checks ignore_errors based on status
      }

      // Check stderr if required
      if step.check_stderr && !output.stderr.is_empty() {
        warn!(
          "Step '{}' produced output on stderr (check_stderr enabled).",
          step.name
        );
        // Return Ok, let caller check ignore_errors based on this condition
        return Ok(output); // Caller checks ignore_errors based on stderr content
      }

      // Success case
      Ok(output)
    }
    Err(e) => Err(e), // Propagate timeout or wait errors
  }
}

// Helper specific for commands, using {{varName}} convention
fn substitute_command_for_validation(
  command_template: &str,
  base_variables: &HashMap<String, String>,
) -> String {
  let mut command = command_template.to_string();
  for (key, value) in base_variables {
    let placeholder = format!("{{{{{}}}}}", key); // Match {{variable_name}}
    command = command.replace(&placeholder, value);
  }
  command
}
