use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};
use std::thread;
use std::time::{Duration, Instant};

use duct::{cmd, Handle};
use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToShoutySnakeCase, ToSnakeCase};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, info, trace, warn};
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
          CaseTransformation::PackageName => {
            // Simple version: lowercase and remove non-alphanumerics
            // More complex might involve splitting by case/separators first
            base_value
              .chars()
              .filter(|c| c.is_ascii_alphanumeric())
              .collect::<String>()
              .to_lowercase()
            // Or alternatively, use snake_case: base_value.to_snake_case()
          }
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
  base_variables: &HashMap<String, String>,
  all_substitutions: &HashMap<String, String>,
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
      all_substitutions
        .get(&vd.placeholder_value)
        .map(|val| (vd.name.clone(), val.clone()))
    })
    .collect::<HashMap<String, String>>();

  let exclude_set: HashSet<String> = manifest.exclude.iter().cloned().collect();

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

    let current_path = entry.path(); // Define current_path here for exclusion check

    if current_path == template_path {
      continue;
    }

    if let Some(entry_name) = current_path.file_name().and_then(|n| n.to_str()) {
      if exclude_set.contains(entry_name) {
        if entry.file_type().is_dir() {
          count_walker.skip_current_dir(); // Skip directory contents if dir is excluded
        }
        // Skip processing this entry entirely (whether file or dir)
        continue;
      }
    }

    let relative_path = match entry.path().strip_prefix(template_path) {
      Ok(p) => p,
      Err(_) => continue,
    };
    let relative_path_str = relative_path.to_string_lossy().to_string();
    let mut skip_entry = false;
    if let Some(condition) = manifest.conditional_paths.get(&relative_path_str) {
      if !evaluate_condition(condition, &base_variables) {
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

    if let Some(entry_name) = current_path.file_name().and_then(|n| n.to_str()) {
      if exclude_set.contains(entry_name) {
        debug!(
          "Excluding entry '{}' based on exclude list.",
          current_path.display()
        );
        if entry.file_type().is_dir() {
          walker.skip_current_dir(); // Skip directory contents if dir is excluded
        }
        // Skip processing this entry entirely (whether file or dir)
        continue;
      }
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
            base_variables,
            all_substitutions,
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
        let content = match fs::read_to_string(current_path) {
          Ok(s) => s,
          Err(e) => {
            // Add specific logging if the error is InvalidData
            if e.kind() == ErrorKind::InvalidData {
              error!(
                      "UTF-8 READ ERROR: Failed to read '{}' as UTF-8 text. Check file encoding or if it should be binary.",
                      current_path.display()
                   );
            } else {
              // Log other IO errors
              error!("IO Error reading '{}': {}", current_path.display(), e);
            }
            // Propagate the original error
            return Err(SpawnError::Io(e));
          }
        };
        let substituted_content = substitute_content(&content, all_substitutions, manifest);
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

/// Performs variable substitution on a single path segment (filename or directory name).
/// It handles both direct variable markers (__VAR_name__) and transformation placeholders.
fn substitute_path_segment(
  segment: &str,
  base_variables: &HashMap<String, String>, // Needed for __VAR_...__ substitution
  all_substitutions: &HashMap<String, String>, // Contains ALL placeholders -> final value
  placeholder_config: &Option<PlaceholderFilenames>,
  variable_definitions: &[VariableDefinition], // Needed to identify __VAR_...__ markers
) -> String {
  // If no placeholder config, no substitution needed for path segments
  let Some(config) = placeholder_config else {
    return segment.to_string();
  };

  let mut current_segment = segment.to_string();

  // --- Pass 1: Handle __VAR_...__ placeholders ---
  // These are substituted with the *base* variable value.
  for var_def in variable_definitions {
    let var_marker = format!("{}{}{}", config.prefix, var_def.name, config.suffix);
    if current_segment.contains(&var_marker) {
      if let Some(base_value) = base_variables.get(&var_def.name) {
        // Replace __VAR_name__ with the raw user input for 'name'
        current_segment = current_segment.replace(&var_marker, base_value);
        trace!(
          "Path Segment Subst (Pass 1): Replaced '{}' with base value '{}'",
          var_marker,
          base_value
        );
      } else {
        warn!(
          "Variable '{}' used in path marker '{}' but not found in base variables map.",
          var_def.name, var_marker
        );
      }
    }
  }

  // --- Pass 2: Handle ALL other placeholders (including transformations and direct base placeholders) ---
  // This uses the pre-computed map of all placeholders (e.g., __PascalName__, --kebab-case--, --base-placeholder--)
  // to their final transformed/base values.
  for (placeholder, final_value) in all_substitutions {
    // Directly replace any remaining occurrences of placeholders from the comprehensive map.
    // This naturally handles transformation placeholders and any direct base placeholders
    // that weren't substituted via the __VAR_ mechanism in Pass 1.
    if current_segment.contains(placeholder) {
      trace!(
        "Path Segment Subst (Pass 2): Replacing placeholder '{}' with value '{}'",
        placeholder,
        final_value
      );
      current_segment = current_segment.replace(placeholder, final_value);
    }
  }

  // Debug log the final result
  if segment != current_segment {
    debug!(
      "Substituted path segment '{}' -> '{}'",
      segment, current_segment
    );
  }

  current_segment
}

/// Executes a validation step command.
pub fn run_command(
  step: &ValidationStep,
  working_dir: &Path,
  base_variables: &HashMap<String, String>,
) -> Result<Output, SpawnError> {
  // 1. Substitute command string
  let substituted_command = substitute_command_for_validation(&step.command, base_variables);

  // 2. Prepare timeout duration
  let timeout_duration = step.timeout_secs.map(Duration::from_secs);

  // 3. Call the execution helper
  let exec_result = execute_command_with_duct(
    &step.name,
    &substituted_command,
    working_dir,
    &step.env,
    timeout_duration,
  );

  // 4. Process the result from the helper (interpret status, stderr, ignore_errors)
  match exec_result {
    Ok(output) => {
      // Includes non-zero exits because of unchecked()
      debug!("Step '{}' executed. Status: {:?}", step.name, output.status);
      if log::log_enabled!(log::Level::Trace) {
        trace!(
          "Step '{}' stdout:\n{}",
          step.name,
          String::from_utf8_lossy(&output.stdout)
        );
        trace!(
          "Step '{}' stderr:\n{}",
          step.name,
          String::from_utf8_lossy(&output.stderr)
        );
      }

      // Check status, respecting ignore_errors
      if !output.status.success() {
        let stderr_string = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
        // Log non-zero exit status correctly
        let status_display = output
          .status
          .code()
          .map(|c| c.to_string())
          .or_else(|| output.status.signal().map(|s| format!("signal {}", s)))
          .unwrap_or_else(|| "unknown".to_string());
        warn!(
          "Step '{}' failed with status: {}. Stderr: {}",
          step.name,
          status_display,
          stderr_string.lines().next().unwrap_or("<empty stderr>")
        );

        // Check if the specific error is "command not found" (127 on Unix)
        // This provides a more specific error message than CommandFailedStatus
        #[cfg(unix)]
        if output.status.code() == Some(127) {
          if !step.ignore_errors {
            return Err(SpawnError::CommandExecError {
              step_name: step.name.clone(),
              source: format!("Command not found (exit code 127): {}", substituted_command).into(),
            });
          } else {
            info!(
              "Ignoring failed status (command not found) for step '{}' (ignore_errors=true).",
              step.name
            );
            // Fall through to check stderr below if needed
          }
        } else {
          // Handle other non-zero exits
          if !step.ignore_errors {
            return Err(SpawnError::CommandFailedStatus {
              step_name: step.name.clone(),
              status: output.status,
              stdout: stdout_string,
              stderr: stderr_string,
            });
          } else {
            info!(
              "Ignoring failed status ({}) for step '{}' (ignore_errors=true).",
              status_display, step.name
            );
          }
        }
        #[cfg(not(unix))] // Fallback for non-unix
        {
          if !step.ignore_errors {
            return Err(SpawnError::CommandFailedStatus {
              step_name: step.name.clone(),
              status: output.status,
              stdout: stdout_string,
              stderr: stderr_string,
            });
          } else {
            info!(
              "Ignoring failed status ({}) for step '{}' (ignore_errors=true).",
              status_display, step.name
            );
          }
        }
      } // end if !output.status.success()

      // Check stderr content, respecting ignore_errors
      // This check runs even if the command failed but ignore_errors=true
      if step.check_stderr && !output.stderr.is_empty() {
        let stderr_string = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
        warn!(
          "Step '{}' produced stderr (check_stderr=true): {}",
          step.name,
          stderr_string.lines().next().unwrap_or("<empty stderr>")
        );
        if !step.ignore_errors {
          // Only fail if the command *also* succeeded OR if status failure was ignored
          if output.status.success() || step.ignore_errors {
            return Err(SpawnError::CommandStderrNotEmpty {
              step_name: step.name.clone(),
              stdout: stdout_string,
              stderr: stderr_string,
            });
          }
          // Otherwise, the CommandFailedStatus error takes precedence
        } else {
          info!(
            "Ignoring non-empty stderr for step '{}' (ignore_errors=true).",
            step.name
          );
        }
      }

      // If we passed the checks or ignored the failures
      info!("Step '{}' considered successful.", step.name);
      Ok(output)
    }
    Err(e) => {
      // Error from execute_command_with_duct (timeout, spawn error, wait error)
      error!("Execution error for step '{}': {}", step.name, e);
      if !step.ignore_errors {
        Err(e) // Propagate the execution error
      } else {
        info!(
          "Ignoring execution error for step '{}' (ignore_errors=true).",
          step.name
        );
        // Construct a dummy error Output when ignoring execution errors
        let exit_status = if cfg!(unix) {
          ExitStatus::from_raw(1) // Use 1 as generic error code
        } else {
          ExitStatus::from_raw(1)
        };

        Ok(Output {
          status: exit_status,
          stdout: Vec::new(),
          stderr: format!("Execution failed and ignored: {}", e).into_bytes(),
        })
      }
    }
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

/// Executes a command using duct, waits for completion (or timeout), then captures output.
/// Duct's capture methods use background threads internally, preventing I/O deadlocks.
/// Executes a command using duct, waits for completion (or timeout), then captures output.
/// Uses duct's internal background threads for capture and unchecked() to get Output on non-zero exit.
fn execute_command_with_duct(
  step_name: &str,
  command_str: &str,
  working_dir: &Path,
  env_overrides: &HashMap<String, String>,
  timeout: Option<Duration>,
) -> Result<Output, SpawnError> {
  info!(
    "Executing (duct unchecked): Step '{}', Command: `{}` in {}",
    step_name,
    command_str,
    working_dir.display()
  );

  // 1. Configure command, including capture and unchecked()
  let mut command_expr = cmd!("sh", "-c", command_str)
    .dir(working_dir)
    .stdout_capture() // Capture stdout - duct reads in background thread
    .stderr_capture() // Capture stderr - duct reads in background thread
    .unchecked(); // <<< --- Add this back! Ensures Ok(Output) on non-zero exit

  // 2. Apply environment overrides iteratively using .env()
  //    This preserves the inherited environment.
  for (key, value) in env_overrides {
    command_expr = command_expr.env(key, value); // Add/override specific vars
  }

  // 2. Start the command, get a handle
  let handle: Handle = match command_expr.start() {
    Ok(h) => h,
    Err(e) => {
      error!("Failed to start command for step '{}': {}", step_name, e);
      if e.kind() == ErrorKind::NotFound {
        // Specific error for command not found during start
        return Err(SpawnError::CommandExecError {
          step_name: step_name.to_string(),
          source: format!("Command/shell not found for step '{}': {}", step_name, e).into(),
        });
      }
      return Err(SpawnError::CommandExecError {
        step_name: step_name.to_string(),
        source: Box::new(e), // Other spawn error
      });
    }
  }; // Make handle mutable for kill()

  // 3. Wait for completion: either blocking wait or polling loop with timeout
  let final_result: Result<Output, SpawnError> = match timeout {
    // --- Case: No Timeout ---
    None => {
      // With unchecked(), wait() returns Ok(Output) or Err(WaitError for non-exit reasons)
      match handle.wait() {
        Ok(output) => {
          debug!(
            "Step '{}' finished (no timeout, unchecked). Status: {:?}",
            step_name, output.status
          );
          Ok(output.clone()) // Includes non-zero exits
        }
        Err(duct_wait_error) => {
          // This is now only for errors *other* than non-zero exit status (e.g., OS error)
          error!(
            "Error waiting (no timeout) for step '{}': {}",
            step_name, duct_wait_error
          );
          Err(SpawnError::CommandExecError {
            // Report as execution error
            step_name: step_name.to_string(),
            source: Box::new(duct_wait_error),
          })
        }
      }
    }
    // --- Case: Timeout ---
    Some(duration) => {
      let start = Instant::now();
      let poll_interval = Duration::from_millis(50); // How often to check

      loop {
        // try_wait() returns Ok(Some(Output)) or Ok(None) or Err(WaitError)
        match handle.try_wait() {
          Ok(Some(output)) => {
            // Process finished within timeout (could be non-zero due to unchecked())
            debug!(
              "Step '{}' finished (timeout loop, unchecked). Status: {:?}",
              step_name, output.status
            );
            break Ok(output.clone());
          }
          Ok(None) => {
            // Process still running, check timer
            if start.elapsed() >= duration {
              // Timeout exceeded
              error!(
                "Step '{}' timed out after {:?}. Killing process.",
                step_name, duration
              );
              if let Err(kill_err) = handle.kill() {
                // Attempt to kill
                warn!(
                  "Failed to kill timed-out process for step '{}': {}",
                  step_name, kill_err
                );
              }
              break Err(SpawnError::CommandExecError {
                // Return timeout error
                step_name: step_name.to_string(),
                source: format!("Step timed out after {} seconds", duration.as_secs()).into(),
              });
            } else {
              // Still within time, sleep a bit
              thread::sleep(poll_interval);
            }
          }
          Err(duct_wait_error) => {
            // Error during try_wait itself (not non-zero exit, but actual wait error)
            error!(
              "Error during try_wait for step '{}': {}",
              step_name, duct_wait_error
            );
            break Err(SpawnError::CommandExecError {
              // Report as execution error
              step_name: step_name.to_string(),
              source: Box::new(duct_wait_error),
            });
          }
        } // end match try_wait
      } // end loop
    } // end Some(duration)
  }; // end match timeout

  // 4. Log final result details (no changes needed here)
  match &final_result {
    Ok(output) => {
      if log::log_enabled!(log::Level::Trace) {
        trace!(
          "Step '{}' final stdout:\n{}",
          step_name,
          String::from_utf8_lossy(&output.stdout)
        );
        trace!(
          "Step '{}' final stderr:\n{}",
          step_name,
          String::from_utf8_lossy(&output.stderr)
        );
      }
    }
    Err(e) => {
      error!(
        "Step '{}' ultimately failed with execution error: {}",
        step_name, e
      );
    }
  }

  final_result // Return the Ok(Output) or Err(SpawnError)
}
