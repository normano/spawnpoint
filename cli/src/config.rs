// src/config.rs
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub variable: String, // Name of the boolean variable
    #[serde(default = "default_condition_value")]
    pub value: String, // Expected value (usually "true" or "false")
}

// Default condition expects the variable to be "true"
fn default_condition_value() -> String { "true".to_string() }

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)] // Good practice to catch typos in yaml
#[serde(rename_all = "camelCase")]
pub struct ScaffoldManifest {
  pub name: String,
  pub description: String,
  pub language: String,
  pub variables: Vec<VariableDefinition>,
  #[serde(default)]
  pub placeholder_filenames: Option<PlaceholderFilenames>,
  #[serde(default)]
  pub binary_extensions: Vec<String>,
  #[serde(default)]
  pub binary_files: Vec<PathBuf>, // Relative to template root
  // --- Conditional Paths ---
  /// Map from relative template path (String) to the condition for inclusion.
  #[serde(default)]
  pub conditional_paths: HashMap<String, Condition>,
  // --- Hooks ---
  #[serde(default)]
  pub pre_generate: Vec<ValidationStep>, // Runs before generation
  #[serde(default)]
  pub post_generate: Vec<ValidationStep>, // Runs after generation
  #[serde(default)]
  pub validation: Option<ValidationConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CaseTransformation {
    PascalCase,     // MyVariable
    CamelCase,      // myVariable
    SnakeCase,      // my_variable
    KebabCase,      // my-variable
    ShoutySnakeCase, // MY_VARIABLE
    PackageName, 
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VariableType {
    String,
    Boolean,
    // Could add Integer, etc. later
}

impl Default for VariableType {
    fn default() -> Self { VariableType::String }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct VariableDefinition {
    pub name: String,
    pub prompt: Option<String>,
    pub placeholder_value: String, // Placeholder for the *original* value
    #[serde(default)]
    pub var_type: VariableType, // Added type hint
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub default: Option<String>,
    /// Defines transformations and the placeholders to use for them.
    #[serde(default)]
    pub transformations: HashMap<CaseTransformation, String>, // e.g., { PascalCase: "__PASCAL_VAR__" }
    #[serde(default)]
    pub validation_regex: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct PlaceholderFilenames {
  #[serde(default = "default_var_prefix")]
  pub prefix: String,
  #[serde(default = "default_var_suffix")]
  pub suffix: String,
}
fn default_var_prefix() -> String {
  "__VAR_".to_string()
}
fn default_var_suffix() -> String {
  "__".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ValidationConfig {
  pub test_variables: HashMap<String, String>,
  #[serde(default)]
  pub setup: Vec<ValidationStep>,
  pub steps: Vec<ValidationStep>,
  #[serde(default)]
  pub teardown: Vec<ValidationStep>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ValidationStep {
  pub name: String,
  pub command: String,
  #[serde(default)]
  pub working_dir: Option<PathBuf>, // Relative to generated dir root
  #[serde(default)]
  pub env: HashMap<String, String>,
  #[serde(default)]
  pub timeout_secs: Option<u64>,
  #[serde(default)]
  pub ignore_errors: bool, // Don't fail validation if this step errors
  #[serde(default)]
  pub always_run: bool, // Primarily for teardown
  #[serde(default)]
  pub check_stderr: bool, // Fail if stderr is not empty
}
