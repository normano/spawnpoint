// src/validate.rs
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use indicatif::ProgressBar;
use log::{debug, error, info};
use tempfile::Builder;

use crate::cli::ValidateArgs;
use crate::config::ValidationStep;
use crate::error::SpawnError;
use crate::generate::find_available_templates;
use crate::utils;

pub fn run_validate(args: ValidateArgs, templates_dir: &Path) -> Result<(), SpawnError> {
  info!(
    "Running validate command for template '{}' (lang: '{}')...",
    args.template, args.language
  );
  debug!(
    "Args: {:?}, Templates Dir: {}",
    args,
    templates_dir.display()
  );

  // --- 1. Find Template & Manifest (REVISED) ---

  // Find all templates first
  let available_templates = find_available_templates(templates_dir)?;

  // Find the specific template matching language and manifest name
  let found_template = available_templates
    .into_iter()
    .find(|(_dir_name, _path, manifest)| {
      manifest.language == args.language && manifest.name == args.template
    });

  let (template_dir_name, template_path, manifest) = match found_template {
    Some(t) => t,
    None => {
      return Err(SpawnError::GenerationError(format!(
        // Use GenerationError for consistency? Or keep specific error?
        "Template '{}' for language '{}' not found.",
        args.template, args.language
      )));
    }
  };
  info!(
    "Found template '{}' in directory {}",
    manifest.name,
    template_path.display()
  );

  // --- Validation Config Check ---
  let validation_config = match &manifest.validation {
    Some(config) => config,
    None => {
      info!(
        "Validation not configured for template '{}'. Skipping.",
        manifest.name
      );
      return Ok(());
    }
  };

  info!("Found validation config for template '{}'", manifest.name);

  // --- 2. Create Temporary Directory ---
  let temp_dir = Builder::new()
    // Use the actual directory name for the prefix, which is likely more filesystem-friendly
    .prefix(&format!("spawnpoint_validate_{}_", template_dir_name))
    .tempdir()
    .map_err(SpawnError::Io)?; // Simplified error mapping
  let temp_path = temp_dir.path();
  info!("Created temporary directory: {}", temp_path.display());

  // --- 2b. Compute Test Variables (Base + Transformed) ---
  // Use validation_config.test_variables as the base map
  let all_test_substitutions = utils::compute_transformed_variables(
    &validation_config.test_variables, // Base vars from test_variables
    &manifest.variables,
  );
  debug!(
    "Computed all test substitutions (keyed by placeholder): {:?}",
    all_test_substitutions
  );

  // --- 3. Generate into Temp Dir ---
  info!("Generating template into temporary directory...");
  utils::copy_template_dir(
    &template_path, // Use the correctly found path
    temp_path,
    &all_test_substitutions,
    &manifest,
  )?;
  info!("Template generation complete.");

  // --- 4. Run Validation Steps ---
  info!("Running validation steps...");
  // Pass the test_variables (base map) for command substitution,
  // as commands likely use the original {{varName}} syntax, not placeholders.
  // Or, update run_command to use the placeholder-keyed map if commands use placeholders. Let's assume commands use {{varName}} for now.
  let result = run_validation_lifecycle(
    validation_config,
    temp_path,
    &validation_config.test_variables,
  );

  // --- 5. Report Result (temp dir cleans up automatically) ---
  match result {
    Ok(_) => {
      info!("✅ Validation successful for template '{}'!", manifest.name);
      Ok(())
    }
    Err(e) => {
      error!("Validation failed for template '{}': {}", manifest.name, e);
      // Propagate the validation error
      Err(e)
    }
  }
}

// --- Helper Functions ---

fn run_validation_lifecycle(
  config: &crate::config::ValidationConfig,
  temp_path: &Path,
  test_variables_for_commands: &HashMap<String, String>,
) -> Result<(), SpawnError> {
  // Calculate total steps once
  let total_steps = config.setup.len() + config.steps.len() + config.teardown.len();
  // Use an AtomicUsize for the shared counter across phases
  let step_counter = AtomicUsize::new(0);

  // Use a hidden progress bar just for println
  let pb = ProgressBar::hidden();

  let original_cwd = std::env::current_dir().map_err(SpawnError::Io)?;

  // --- Setup Steps ---
  // Run relative to original CWD by default
  let setup_result = execute_phase_steps(
    "Setup",
    &config.setup,
    &original_cwd,
    temp_path, // Pass temp_path for potential workingDir resolution
    test_variables_for_commands,
    &pb,
    &step_counter,
    total_steps,
  );
  if let Err(e) = setup_result {
    return Err(e);
  } // Exit early on setup failure

  // --- Main Validation Steps ---
  // Run relative to temp_path by default
  let validation_result = execute_phase_steps(
    "Validation",
    &config.steps,
    temp_path, // Default base is temp_path
    temp_path, // Pass temp_path for potential workingDir resolution
    test_variables_for_commands,
    &pb,
    &step_counter,
    total_steps,
  );
  // Don't return early on validation failure yet, need to run teardown if applicable

  // --- Teardown Steps ---
  let mut teardown_result = Ok(()); // Track teardown result separately
  if !config.teardown.is_empty() {
    pb.println("--- Running Teardown phase ---".to_string());
    for step in &config.teardown {
      let current_step_num = step_counter.fetch_add(1, Ordering::SeqCst) + 1;
      let base_path = &original_cwd;
      // Allow workingDir relative to temp_path even for teardown
      let run_path = step
        .working_dir
        .as_ref()
        .map_or(base_path.clone(), |wd| temp_path.join(wd));

      // Run teardown if always_run is true OR if validation phase succeeded
      if step.always_run || validation_result.is_ok() {
        pb.println(format!(
          "[{}/{}] Running teardown step: '{}'{}...",
          current_step_num,
          total_steps,
          step.name,
          if step.always_run && validation_result.is_err() {
            " (always_run)"
          } else {
            ""
          }
        ));
        match utils::run_command(step, &run_path, test_variables_for_commands) {
          Ok(output) => {
            if !output.status.success() && !step.ignore_errors {
              pb.println(format!(
                "❌ Teardown step '{}' failed (status: {:?}).",
                step.name,
                output.status.code()
              ));
              if teardown_result.is_ok() {
                // Only store the first teardown error
                teardown_result = Err(SpawnError::CommandFailedStatus {
                  step_name: format!("Teardown: {}", step.name), // Add context
                  status: output.status,
                  stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                  stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
              }
            } else if step.check_stderr && !output.stderr.is_empty() && !step.ignore_errors {
              pb.println(format!(
                "❌ Teardown step '{}' failed (check_stderr=true).",
                step.name
              ));
              if teardown_result.is_ok() {
                teardown_result = Err(SpawnError::CommandStderrNotEmpty {
                  step_name: format!("Teardown: {}", step.name), // Add context
                  stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                  stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
              }
            } else {
              pb.println(format!("✅ Teardown step '{}' successful.", step.name));
            }
          }
          Err(e) => {
            pb.println(format!(
              "❌ Teardown step '{}' execution error: {}",
              step.name, e
            ));
            if teardown_result.is_ok() && !step.ignore_errors {
              teardown_result = Err(e);
            }
          }
        }
      } else {
        pb.println(format!(
          "[{}/{}] Skipping teardown step '{}' (always_run=false and validation failed).",
          current_step_num, total_steps, step.name
        ));
      }
    }
    pb.println("--- Finished Teardown phase ---".to_string());
  }

  // Final result prioritizes setup/validation errors over teardown errors
  validation_result.and(teardown_result)
}

/// Executes a sequence of validation steps for a given phase.
/// Returns Ok(()) if all non-ignored steps succeed, or the first critical Err encountered.
fn execute_phase_steps(
  phase_name: &str,
  steps: &[ValidationStep],
  default_base_dir: &Path, // Base path (e.g., original CWD or temp dir)
  temp_path: &Path,        // Always pass temp_path for resolving potential workingDir overrides
  test_variables_for_commands: &HashMap<String, String>,
  pb: &ProgressBar,           // Pass progress bar for printing
  step_counter: &AtomicUsize, // Shared counter
  total_steps: usize,
) -> Result<(), SpawnError> {
  // Return Result to propagate errors
  if steps.is_empty() {
    return Ok(());
  }

  pb.println(format!("--- Running {} phase ---", phase_name));
  for step in steps {
    // Increment counter *before* running the step
    let current_step_num = step_counter.fetch_add(1, Ordering::SeqCst) + 1;

    // Determine working directory: use step's if specified (relative to temp_path), else use default_base_dir
    let run_path = step
      .working_dir
      .as_ref()
      // If working_dir is specified in the step, it's relative to the temp_path.
      // Otherwise, use the default_base_dir passed for the phase.
      .map_or(default_base_dir.to_path_buf(), |wd| temp_path.join(wd));

    pb.println(format!(
      "[{}/{}] Running step: '{}'...",
      current_step_num, total_steps, step.name
    ));

    match utils::run_command(step, &run_path, test_variables_for_commands) {
      Ok(output) => {
        // Check status AFTER command runs
        if !output.status.success() {
          let stderr_string = String::from_utf8_lossy(&output.stderr).to_string();
          let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
          pb.println(format!(
            "❌ Step '{}' failed (status: {:?}).",
            step.name,
            output.status.code()
          ));
          if !step.ignore_errors {
            // CONSTRUCT THE ERROR INSTANCE
            return Err(SpawnError::CommandFailedStatus {
              step_name: step.name.clone(),
              status: output.status,
              stdout: stdout_string,
              stderr: stderr_string,
            });
          } else {
            pb.println(format!("   (Ignoring error for step '{}')", step.name));
          }
        } else if step.check_stderr && !output.stderr.is_empty() {
          let stderr_string = String::from_utf8_lossy(&output.stderr).to_string();
          let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
          pb.println(format!(
            "❌ Step '{}' failed (check_stderr=true, stderr not empty).",
            step.name
          ));
          if !step.ignore_errors {
            // CONSTRUCT THE ERROR INSTANCE
            return Err(SpawnError::CommandStderrNotEmpty {
              step_name: step.name.clone(),
              stdout: stdout_string,
              stderr: stderr_string,
            });
          } else {
            pb.println(format!("   (Ignoring stderr for step '{}')", step.name));
          }
        } else {
          pb.println(format!("✅ Step '{}' successful.", step.name));
        }
      }
      Err(e) => {
        // Execution errors (spawn, timeout, wait) - run_command returns these directly now
        pb.println(format!("❌ Step '{}' execution error: {}", step.name, e));
        if !step.ignore_errors {
          return Err(e); // Propagate the execution error (already SpawnError::CommandExecError)
        } else {
          pb.println(format!(
            "   (Ignoring execution error for step '{}')",
            step.name
          ));
        }
      }
    }
  }
  pb.println(format!("--- Finished {} phase ---", phase_name));
  Ok(())
}
