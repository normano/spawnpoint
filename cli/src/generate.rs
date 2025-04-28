// src/generate.rs
use crate::cli::GenerateArgs;
use crate::config::{ScaffoldManifest, ValidationStep, VariableType};
use crate::error::SpawnError;
use crate::list::read_and_parse_manifest;
use crate::utils;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

#[cfg(feature = "regex")] // Conditionally compile regex logic
use regex::Regex;

pub fn run_generate(args: GenerateArgs, templates_dir: &Path) -> Result<(), SpawnError> {
  info!("Running generate command...");
  debug!(
    "Args: {:?}, Templates Dir: {}",
    args,
    templates_dir.display()
  );

  // --- 1. Select Template ---
  let (template_name, template_path, manifest) =
    select_template(args.language, args.template, templates_dir)?;
  info!(
    "Selected template: '{}' from {}",
    template_name,
    template_path.display()
  );
  debug!("Manifest loaded: {:?}", manifest);

  // --- 2. Gather Variables ---
  let base_variables = gather_variables(&manifest)?;
  debug!("Gathered base variables: {:?}", base_variables);

  // --- 2b. Compute All Variables (Base + Transformed) ---
  let all_substitutions =
    utils::compute_transformed_variables(&base_variables, &manifest.variables);
  debug!(
    "Computed all substitutions (keyed by placeholder): {:?}",
    all_substitutions
  );

  // --- 3. Run Pre-Generate Hooks ---
  let original_cwd = env::current_dir().map_err(SpawnError::Io)?;
  info!("Checking for pre-generate hooks...");
  run_hooks(
    "Pre-Generate",
    &manifest.pre_generate,
    &base_variables, // Pass base vars for {{varName}} substitution in commands
    &original_cwd,   // Hooks run relative to original CWD by default
  )?;
  info!("Pre-generate hooks finished.");

  // --- 4. Prepare Output Directory ---
  let output_path = &args.output_dir;
  if !output_path.exists() {
    fs::create_dir_all(output_path).map_err(|e| SpawnError::OutputDirCreation {
      path: output_path.to_path_buf(),
      source: e,
    })?;
    info!("Created output directory: {}", output_path.display());
  } else if !output_path.is_dir() {
    return Err(SpawnError::GenerationError(format!(
      "Output path '{}' exists but is not a directory.",
      output_path.display()
    )));
  } else {
    // Optional: Check if directory is empty and warn/prompt?
    // For now, we'll overwrite/add files.
    warn!(
      "Output directory '{}' already exists. Files may be overwritten.",
      output_path.display()
    );
  }

  // --- 5. Generate Project ---
  info!("Generating project files...");
  utils::copy_template_dir(
    &template_path,
    output_path,
    &base_variables,
    &all_substitutions,
    &manifest,
  )?;

  info!(
    "Successfully generated project in '{}'!",
    output_path.display()
  );

  // --- 6. Run Post-Generate Hooks ---
  info!("Checking for post-generate hooks...");
  run_hooks(
    "Post-Generate",
    &manifest.post_generate,
    &base_variables, // Pass base vars for {{varName}} substitution in commands
    output_path,     // Hooks run relative to the generated output path by default
  )?;
  info!("Post-generate hooks finished.");

  Ok(())
}

// --- Helper Functions ---

// Helper to execute a list of hook steps.
fn run_hooks(
  phase_name: &str, // "Pre-Generate" or "Post-Generate"
  hooks: &[ValidationStep],
  variables: &HashMap<String, String>, // Base variables for {{varName}} command substitution
  default_base_dir: &Path,             // Default directory to run hook in
) -> Result<(), SpawnError> {
  if hooks.is_empty() {
    return Ok(());
  }

  info!("--- Running {} phase ---", phase_name);
  for (i, step) in hooks.iter().enumerate() {
    let step_num = i + 1;
    let total_steps = hooks.len();

    // Determine working directory: use step's if specified (relative to default), else use default
    let run_path = step
      .working_dir
      .as_ref()
      .map_or(default_base_dir.to_path_buf(), |wd| {
        default_base_dir.join(wd).to_path_buf()
      });
    // Need to handle potential non-existence of default_base_dir.join(wd) if needed,
    // but run_command should handle CWD errors. Using owned path now.

    info!(
      "[{}/{}] Running step: '{}'...",
      step_num, total_steps, step.name
    );

    // Execute the command using the *base* variables map for substitution
    match utils::run_command(step, &run_path, variables) {
      Ok(output) => {
        // Check status AFTER command runs
        if !output.status.success() {
          let stderr_string = String::from_utf8_lossy(&output.stderr).to_string();
          let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
          error!(
            "{} hook step '{}' failed (status: {:?}).\nStderr:\n{}\nStdout:\n{}",
            phase_name, step.name, output.status, stderr_string, stdout_string
          );
          if !step.ignore_errors {
            // Return specific error for hook failure
            return Err(SpawnError::CommandFailedStatus {
              step_name: format!("{} Hook: {}", phase_name, step.name), // Add phase context
              status: output.status,
              stdout: stdout_string,
              stderr: stderr_string,
            });
          } else {
            warn!(
              "Ignoring failed status in {} hook step '{}' (ignore_errors=true).",
              phase_name, step.name
            );
          }
        } else if step.check_stderr && !output.stderr.is_empty() {
          let stderr_string = String::from_utf8_lossy(&output.stderr).to_string();
          let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
          error!(
            "{} hook step '{}' check_stderr failed.\nStderr:\n{}\nStdout:\n{}",
            phase_name, step.name, stderr_string, stdout_string
          );
          if !step.ignore_errors {
            return Err(SpawnError::CommandStderrNotEmpty {
              step_name: format!("{} Hook: {}", phase_name, step.name),
              stdout: stdout_string,
              stderr: stderr_string,
            });
          } else {
            warn!(
              "Ignoring non-empty stderr in {} hook step '{}' (ignore_errors=true).",
              phase_name, step.name
            );
          }
        } else {
          info!(
            "[{}/{}] Step '{}' successful.",
            step_num, total_steps, step.name
          );
        }
      }
      Err(e) => {
        // Execution errors (spawn, timeout, wait)
        error!(
          "{} hook step '{}' execution error: {}",
          phase_name, step.name, e
        );
        if !step.ignore_errors {
          // Wrap the original error if possible, or create a new one
          // Reusing CommandExecError might require adjusting its structure or creating a new HookExecError variant
          return Err(SpawnError::CommandExecError {
            step_name: format!("{} Hook: {}", phase_name, step.name),
            source: format!("Execution failed: {}", e).into(), // Simple wrapping for now
          });
        } else {
          warn!(
            "Ignoring execution error in {} hook step '{}' (ignore_errors=true).",
            phase_name, step.name
          );
        }
      }
    }
  }
  info!("--- Finished {} phase ---", phase_name);
  Ok(())
}

fn select_template(
  lang_opt: Option<String>,
  template_opt: Option<String>,
  templates_dir: &Path,
) -> Result<(String, PathBuf, ScaffoldManifest), SpawnError> {
  let available_templates = find_available_templates(templates_dir)?;

  if available_templates.is_empty() {
    return Err(SpawnError::GenerationError(
      "No templates found.".to_string(),
    ));
  }

  match (lang_opt, template_opt) {
    // Both provided: Find exact match
    (Some(lang), Some(template_name)) => {
      // template_name here comes from the user argument
      log::debug!(
        "Attempting to find exact match: lang='{}', template_name='{}'",
        lang,
        template_name
      );
      log::debug!(
        "Available templates before find: {:?}",
        available_templates
          .iter()
          .map(|(_dir_name, p, m)| (&m.name, p.display(), &m.language))
          .collect::<Vec<_>>()
      ); // Log manifest name

      available_templates
        .into_iter()
        .find(|(_dir_name, _path, manifest)| {
          // Ignore dir_name from tuple here
          let lang_match = manifest.language == lang;
          // Compare against the manifest's name field!
          let name_match = manifest.name == template_name;
          log::trace!(
            "Checking manifest '{}': lang_match={}, name_match={}",
            manifest.name,
            lang_match,
            name_match
          );
          lang_match && name_match
        })
        .ok_or_else(|| {
          SpawnError::GenerationError(format!(
            "Template '{}' for language '{}' not found.",
            template_name, lang
          ))
        })
    }
    // Only language provided: Select template from language
    (Some(lang), None) => {
      let lang_templates: Vec<_> = available_templates
        .into_iter()
        .filter(|(_, _, manifest)| manifest.language == lang)
        .collect();

      if lang_templates.is_empty() {
        return Err(SpawnError::GenerationError(format!(
          "No templates found for language '{}'.",
          lang
        )));
      }
      if lang_templates.len() == 1 {
        Ok(lang_templates.into_iter().next().unwrap())
      } else {
        let names: Vec<&str> = lang_templates
          .iter()
          .map(|(_, _, manifest)| manifest.name.as_str())
          .collect();
        let selection = Select::with_theme(&ColorfulTheme::default())
          .with_prompt(format!("Select a template for language '{}'", lang))
          .items(&names)
          .default(0)
          .interact()?;
        Ok(lang_templates.into_iter().nth(selection).unwrap()) // Should always succeed
      }
    }
    // Only template name provided: Ambiguous - error or try to find unique? Let's error for now.
    (None, Some(template_name)) => {
      log::debug!(
        "Attempting to find template by name only: template_name='{}'",
        template_name
      );
      let matches: Vec<_> = available_templates
        .into_iter()
        // Compare against manifest.name here too
        .filter(|(_dir_name, _path, manifest)| manifest.name == template_name)
        .collect();
      if matches.len() == 1 {
        Ok(matches.into_iter().next().unwrap())
      } else if matches.is_empty() {
        Err(SpawnError::GenerationError(format!(
          "Template '{}' not found.",
          template_name
        )))
      } else {
        Err(SpawnError::GenerationError(format!(
              "Template name '{}' is ambiguous (found in multiple languages), please specify a language with --language.", template_name
          )))
      }
    }
    // Neither provided: Select language, then template
    (None, None) => {
      let mut languages: Vec<String> = available_templates
        .iter()
        .map(|(_, _, manifest)| manifest.language.clone())
        .collect();
      languages.sort();
      languages.dedup();

      if languages.is_empty() {
        return Err(SpawnError::GenerationError(
          "No templates found.".to_string(),
        ));
      }

      let lang_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select the language/framework")
        .items(&languages)
        .default(0)
        .interact()?;
      let selected_lang = &languages[lang_selection];

      // Now select template within that language
      let lang_templates: Vec<_> = available_templates
        .into_iter()
        .filter(|(_, _, manifest)| &manifest.language == selected_lang)
        .collect();

      if lang_templates.len() == 1 {
        Ok(lang_templates.into_iter().next().unwrap())
      } else {
        let names: Vec<&str> = lang_templates
          .iter()
          .map(|(name, _, _)| name.as_str())
          .collect();
        let selection = Select::with_theme(&ColorfulTheme::default())
          .with_prompt(format!(
            "Select a template for language '{}'",
            selected_lang
          ))
          .items(&names)
          .default(0)
          .interact()?;
        Ok(lang_templates.into_iter().nth(selection).unwrap())
      }
    }
  }
}

pub(crate) fn find_available_templates(
  templates_dir: &Path,
) -> Result<Vec<(String, PathBuf, ScaffoldManifest)>, SpawnError> {
  let mut templates = Vec::new();
  if !templates_dir.is_dir() {
    warn!(
      "Templates directory not found or is not a directory: {}",
      templates_dir.display()
    );
    return Ok(templates); // Return empty vec if dir doesn't exist
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
      let template_name = path
        .file_name()
        .map_or_else(|| ".".into(), |n| n.to_string_lossy().to_string());

      if manifest_path.is_file() {
        match read_and_parse_manifest(&manifest_path) {
          Ok(manifest) => {
            templates.push((template_name, path.clone(), manifest));
          }
          Err(e) => {
            warn!(
              "Skipping directory '{}': Could not read or parse scaffold.yaml: {}",
              template_name, e
            );
          }
        }
      } else {
        debug!(
          "Directory {} does not contain scaffold.yaml.",
          path.display()
        );
      }
    }
  }
  Ok(templates)
}

fn gather_variables(manifest: &ScaffoldManifest) -> Result<HashMap<String, String>, SpawnError> {
  let mut variables = HashMap::new();
  println!("Please provide values for the following variables:");

  for var_def in &manifest.variables {
    let Some(prompt) = &var_def.prompt else {
      continue;
    };
    let default_val_str = var_def.default.as_deref();

    let theme = ColorfulTheme::default();
    let value = match var_def.var_type {
      VariableType::Boolean => {
        let default_bool = default_val_str.map_or(false, |s| s.eq_ignore_ascii_case("true"));
        Confirm::with_theme(&theme)
          .with_prompt(prompt)
          .default(default_bool)
          .interact()?
          .to_string() // Store as "true" or "false"
      }
      VariableType::String => {
        if var_def.sensitive {
          let input = Password::with_theme(&theme).with_prompt(prompt);
          // Password doesn't support default display, maybe confirm?
          // For now, no default for password.
          input.interact()?
        } else {
          let mut input = Input::with_theme(&theme).with_prompt(prompt);
          if let Some(default_val) = default_val_str {
            input = input.default(default_val.to_string());
          }

          // --- Add Validation ---
          #[cfg(feature = "regex")] // Only include if regex feature is enabled
          if let Some(regex_str) = &var_def.validation_regex {
            match Regex::new(regex_str) {
              Ok(regex) => {
                let regex_err_msg = format!("Input must match regex: {}", regex_str);
                input = input.validate_with(move |input: &String| -> Result<(), String> {
                  if regex.is_match(input) {
                    Ok(())
                  } else {
                    Err(regex_err_msg.clone())
                  }
                });
              }
              Err(e) => {
                // Log error if regex is invalid in the manifest, but don't block generation
                warn!(
                  "Invalid validation_regex for variable '{}': {} - Skipping validation.",
                  var_def.name, e
                );
              }
            }
          }
          // --- End Validation ---

          input.interact_text()?
        }
      } // Add other types later if needed
    };
    variables.insert(var_def.name.clone(), value);
  }
  Ok(variables)
}
