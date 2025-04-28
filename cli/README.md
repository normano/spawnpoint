# Spawn Point (`spawnpoint`) CLI ✨

**Spawn Point (`spawnpoint`)** is a fast, flexible, and robust command-line tool for generating project scaffolds from templates. Built in Rust, it focuses on ensuring template quality and maintainability through an integrated validation system.

**(Work in Progress - Expect changes and rapid development!)**

## Why Spawn Point?

Creating new projects consistently can be tedious. Scaffolding tools help, but templates often break silently over time ("template rot"). Spawn Point addresses this with:

1.  **Speed:** Built in Rust for fast file generation.
2.  **Flexibility:** Uses a simple placeholder-value system, keeping templates as valid projects. Supports transformations, conditional files, and hooks.
3.  **⭐ Integrated Validation:** The `validate` command runs build/test steps defined *in the template manifest*, ensuring templates produce working code *before* you use them. This drastically improves template reliability and maintainability.

## Features

*   **Template Listing:** Discover available templates.
*   **Interactive & Non-Interactive Generation:** Generate projects via prompts or CLI flags.
*   **Integrated Template Validation:** Verify templates generate working projects.
*   **Placeholder Value Substitution:** Replaces specific placeholder strings in files.
*   **Variable Transformations:** Auto-generates `PascalCase`, `kebab-case`, etc., from input.
*   **Variable Input Validation (via optional regex feature)**
*   **Filename/Directory Substitution:** Renames files/dirs based on variables.
*   **Conditional File Generation:** Include/exclude files/dirs based on boolean variables.
*   **Excluding Files/Directories:** Exclude specific files or directories by name (e.g., `target`, `.git`, `.DS_Store`) from template processing.
*   **Pre/Post Generation Hooks:** Run custom commands during generation.
*   **Cross-Platform:** Built with Rust.

## Installation

*(Instructions to be added once build/release process is defined. Typically involves `cargo install spawn-point` or downloading binaries.)*

```bash
# Example (replace with actual instructions later)
# Install from source after cloning the repository:
cargo install --path .
# OR
# Download binary from releases page...
```

## Usage

The main command is `spawnpoint`.

```
spawnpoint [OPTIONS] <COMMAND>
```

**Commands:**

*   `list`: List available project templates.
*   `generate`: Generate a new project from a template.
*   `validate`: Validate that a template generates a working project.

**Common Options:**

*   `-v, --verbose`: Increase output verbosity (e.g., `-v` for info, `-vv` for debug, `-vvv` for trace).
*   `--templates-dir <PATH>`: Specify a custom directory containing templates (overrides default locations and `SPAWNPOINT_TEMPLATES_DIR` env var).
*   `-h, --help`: Print help information.
*   `--version`: Print version information.

---

### Locating Templates

Spawn Point needs to find where your template directories are stored. It searches in the following locations, using the first valid directory it finds:

1.  **`--templates-dir <PATH>` (CLI Flag):** The path provided via the command-line argument. This always takes precedence.
2.  **`SPAWNPOINT_TEMPLATES_DIR` (Environment Variable):** The path specified in this environment variable.
3.  **User Configuration Directory:** A `templates` subdirectory within the standard user configuration location. This is the recommended place for users to store their templates after installing `spawnpoint`. Paths vary by OS:
    *   **Linux:** `$XDG_CONFIG_HOME/spawnpoint/templates` (often `~/.config/spawnpoint/templates`)
    *   **macOS:** `~/Library/Application Support/spawnpoint/templates`
    *   **Windows:** `%APPDATA%\spawnpoint\templates` (e.g., `C:\Users\Username\AppData\Roaming\spawnpoint\templates`)
4.  **Executable-Relative Directory:** A `templates` subdirectory located in the same directory as the `spawnpoint` executable itself. Useful for portable distributions or development setups where templates are bundled.
5.  **Current Working Directory (CWD):** A `templates` subdirectory within the directory where you run the `spawnpoint` command. This is the last resort and primarily useful during development when working directly inside the `spawnpoint` project repository.

If no valid directory is found in any of these locations, commands like `list` or `generate` will report an error or find no templates.

---

### `spawnpoint list`

Displays discovered templates from the determined templates directory.

**Example:**

```bash
spawnpoint list
```

**Output:**

```
Available Spawn Point Templates:
Name                      | Language      | Description
--------------------------|---------------|-----------------------------------------------------------------
Node.js Base v1           | nodejs        | Minimal Node.js/TypeScript project setup.
Java Gradle CLI App v1    | java          | A basic Java command-line application using Gradle.
Java Maven CLI App v1     | java          | A basic Java command-line application using Maven.
Rust CLI App v1           | rust          | A basic command-line application written in Rust using Clap.
Rust Leptos CSR App v1    | rust-leptos   | A basic client-side rendered web application using Rust and Leptos.
# ... other templates
```

---

### `spawnpoint generate`

Generates a new project. Can be run interactively or with flags.

**Options:**

*   `-l, --language <LANG>`: Specify the language/framework of the template (e.g., `nodejs`, `rust`). Skips language selection prompt.
*   `-t, --template <NAME>`: Specify the exact template name (must match the `name` in `scaffold.yaml`). Skips template selection prompt.
*   `-o, --output-dir <PATH>`: Directory to generate the project into (defaults to current directory `.`).
*   *(Planned: Flags to provide variables non-interactively, e.g., `--var name=value`)*

**Examples:**

1.  **Fully Interactive:**
    ```bash
    spawnpoint generate
    ```
    *   Prompts you to select language.
    *   Prompts you to select template within that language.
    *   Prompts for each variable defined in the chosen template's `scaffold.yaml`.
    *   Generates files in the current directory.

2.  **Specify Template, Interactive Variables:**
    ```bash
    spawnpoint generate -l rust -t "Rust CLI App v1" -o ./my-new-rust-cli
    ```
    *   Finds the specified template.
    *   Prompts for variables (`crateName`, `crateDescription`, etc.).
    *   Generates files in `./my-new-rust-cli`.

3.  **Specify Output Directory Only:**
    ```bash
    spawnpoint generate -o ./output
    ```
    *   Prompts for language, template, and variables.
    *   Generates files in `./output`.

---

### `spawnpoint validate`

Validates a specific template by generating it in a temporary location and running predefined commands (install, build, test, etc.) from its `scaffold.yaml`. This is crucial for template maintainers.

**Arguments:**

*   `<LANGUAGE>`: The language identifier of the template (e.g., `nodejs`, `rust`).
*   `<TEMPLATE>`: The exact name of the template (from `scaffold.yaml`, e.g., `"Node.js Base v1"`).

**Example:**

```bash
# Validate the basic Rust CLI template
spawnpoint validate rust "Rust CLI App v1"

# Validate the Java Gradle template with more detailed output
# Uses 'java' as the language identifier from its scaffold.yaml
spawnpoint -vv validate java "Java Gradle CLI App v1"
```

**How Validation Works:**

1.  Finds the specified template using the standard [Locating Templates](#locating-templates) logic.
2.  Reads the `validation` section in its `scaffold.yaml`.
3.  Creates a secure temporary directory.
4.  Generates the template into the temp directory using the `testVariables` defined in the manifest (no interactive prompts). **Excludes files/directories listed in `exclude`.**
5.  Executes `setup` commands (if any).
6.  **Executes `steps` commands sequentially inside the temp directory.** These usually include:
    *   Dependency installation (`npm install`, `cargo build`, `gradle assemble`, etc.)
    *   Linting/Formatting checks (`eslint`, `cargo fmt --check`, etc.)
    *   Build commands (`npm run build`, `cargo build --release`, etc.)
    *   Tests (`npm test`, `cargo test`, `gradle test`, etc.)
7.  Checks the exit code (and optionally stderr) of each step. If a non-ignored step fails, validation fails.
8.  Executes `teardown` commands (if any), even if previous steps failed (if `alwaysRun: true`).
9.  **Note:** Validation steps inherit the environment (including `PATH`) from `spawnpoint` by default. You can add or override variables using the `env` map within a specific `ValidationStep`.
10. Reports overall success or failure. The temporary directory is automatically cleaned up.

**Benefits:** This ensures that templates stay functional and produce working projects as dependencies and best practices evolve. It's a crucial tool for template maintainers.

---

## Example Templates Included

This tool comes with several example templates to demonstrate its capabilities:

*   **`Node.js Base v1` (`nodejs`):** A minimal Node.js/TypeScript setup. Demonstrates basic substitution, transformations (`kebabCase`, `PascalCase`), conditional files (`Dockerfile`), and hooks (`git init`).
*   **`Rust CLI App v1` (`rust`):** A simple Rust CLI using `clap`. Shows Rust-specific validation steps (`cargo fmt`, `clippy`, `build`, `test`). Includes `exclude: [ "target" ]`.
*   **`Java Gradle CLI App v1` (`java`):** A standard Java CLI project using Gradle. Demonstrates Java project structure, Gradle validation, and filename placeholders for package structure.
*   **`Java Maven CLI App v1` (`java`):** A standard Java CLI project using Maven.
*   **`Rust Leptos CSR App v1` (`rust-leptos`):** A basic client-side rendered Leptos web app. Shows WASM build validation using `wasm-pack`.

Explore the `templates/` directory (found via the [Locating Templates](#locating-templates) logic) and their `scaffold.yaml` files to see how they are configured.

## Creating Your Own Templates

1.  Create a new directory for your template. The recommended location is within the user configuration directory (see [Locating Templates](#locating-templates)), e.g., `~/.config/spawnpoint/templates/my-python-api`.
2.  Add your project files. Use unique strings (e.g., `--my-placeholder--`) where values need to be replaced. **Do not include build artifact directories like `target/`, `node_modules/`, `dist/`, etc.**
3.  Create a `scaffold.yaml` file in the root of your template directory.
4.  Define `name`, `description`, `language`.
5.  Define `variables` with `name`, `prompt`, and the exact `placeholderValue` used in your files. Add `transformations` if needed. Add `validation_regex` for input validation if desired (requires `regex` feature).
6.  Configure `placeholderFilenames`, `conditionalPaths`, `preGenerate`, `postGenerate` as required.
7.  Configure `binaryExtensions` (e.g., `.png`, `.lock`) and `binaryFiles` (e.g., `.DS_Store`) for files that should be copied without processing content.
8.  Configure `exclude` with a list of file or directory names (e.g., `target`, `.git`, `.mypy_cache`) that should be completely ignored during generation. This is primarily for ignoring files/directories that might accidentally be present in the template source but shouldn't be copied.
9.  **Crucially, add a `validation` section:**
    *   Define `testVariables` with realistic values for testing.
    *   Define `env` maps within steps if specific environment variables are needed (otherwise the parent environment is inherited).
    *   Define `steps` that install dependencies, build, lint, and test the generated project. Use flags like `--no-daemon` for tools like Gradle if needed.
10. Test your template using `spawnpoint validate <lang> "<Your Template Name>"`.
11. Test generation using `spawnpoint generate ...`.

---

## Development

This section provides instructions for developing `spawnpoint` itself.

**Prerequisites:**

*   **Rust Toolchain:** Ensure you have Rust installed. You can get it from [rustup.rs](https://rustup.rs/).

**Building:**

1.  Clone the repository:
    ```bash
    git clone <repository-url>
    cd spawnpoint # Or your repo name
    ```
2.  Build the project:
    *   For a development build:
        ```bash
        cargo build
        ```
    *   For an optimized release build:
        ```bash
        cargo build --release
        ```
    The executable will be located in `target/debug/spawnpoint` or `target/release/spawnpoint`.

**Running Locally:**

You can run the development version directly using `cargo run`. Any arguments after `--` will be passed directly to `spawnpoint`:

```bash
# Run the list command (using templates in ./templates/ if it exists)
cargo run -- list

# Generate using a specific template directory
cargo run -- --templates-dir /path/to/your/templates generate -l rust -t "My Dev Template"

# Validate a template with high verbosity
cargo run -- -vvv --templates-dir /path/to/your/templates validate rust "My Dev Template"
```

**Testing:**

Run the test suite using:

```bash
cargo test
```

**Tips for Development:**

*   **Local Templates:** Use the `--templates-dir path/to/your/test/templates` flag frequently to test against templates you are developing locally without needing to install them globally or rely on default locations.
*   **Verbosity:** Use `-v`, `-vv`, or `-vvv` flags with `cargo run -- ...` to get detailed logging output, which is helpful for debugging generation or validation issues.
*   **Enable Features:** If testing features behind feature flags (like `regex`), use `cargo run --features regex -- ...` or `cargo build --features regex`.

---