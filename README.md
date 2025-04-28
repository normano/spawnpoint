# Spawn Point ✨

**Spawn Point (`spawnpoint`)** is an ambitious, high-performance command-line tool designed to streamline the creation of new projects from predefined templates, with a strong focus on **template validity and maintainability**. Built in Rust, it aims to be fast, reliable, and flexible.

**(Work in Progress - Expect changes and rapid development!)**

## The Problem

Starting new projects often involves copying boilerplate code, renaming files, replacing placeholder values, and running initial setup commands. Many scaffolding tools exist, but they often face challenges:

1.  **Template Rot:** Templates easily become outdated or broken as dependencies and best practices evolve. It's often hard to know if a template will actually produce a working project *before* generating it.
2.  **Inflexible Markers:** Relying solely on syntax like `{{variable}}` can sometimes break the template code itself or make it hard to have a template that is *also* a valid, runnable project in its default state.
3.  **Manual Validation:** Template authors typically rely on external CI setups to validate their templates, which isn't integrated into the scaffolding tool itself.

## The Spawn Point Solution

Spawn Point tackles these issues by providing:

*   **Fast Project Generation:** Leverages Rust's performance for quick file operations and substitutions.
*   **Flexible Templating:** Uses a simple but powerful placeholder value system, allowing templates to remain valid projects themselves. Supports variable transformations (case changes) and filename/directory substitutions.
*   **Conditional Logic:** Include or exclude files and directories based on user choices during generation.
*   **Lifecycle Hooks:** Run custom commands before (`preGenerate`) and after (`postGenerate`) file generation for tasks like `git init` or dependency installation.
*   **⭐ Integrated Template Validation:** This is a key differentiator! Spawn Point includes a `spawnpoint validate <lang> <template>` command that:
    *   Generates the template into a temporary directory using predefined test variables.
    *   Executes a series of validation steps (**install**, **build**, **lint**, **test**, etc.) defined *within the template's manifest (`scaffold.yaml`)*.
    *   Reports success or failure based on the outcome of these steps.
    *   **Benefit:** Template authors and maintainers can easily verify that their templates produce working, buildable, and testable projects directly within the Spawn Point ecosystem, significantly reducing template rot and increasing confidence.

## Core Features

*   **Template Listing:** `spawnpoint list` - Discover available project templates.
*   **Interactive Generation:** `spawnpoint generate` - Guides users through template selection and variable input via prompts. Supports non-interactive generation via flags (planned).
*   **Template Validation:** `spawnpoint validate <lang> <template>` - Runs predefined build/test steps against a template to ensure it generates a valid project.
*   **Placeholder System:** Uses unique but syntactically valid placeholder *values* in template files (e.g., `"--placeholder-value--"`) mapped in `scaffold.yaml`, avoiding syntax conflicts.
*   **Variable Input Validation (via optional regex feature)**
*   **Variable Transformations:** Automatically generates different variable casings (PascalCase, kebab-case, etc.) from a single user input.
*   **Filename/Directory Substitution:** Renames files and directories based on variables (e.g., `__VAR_componentName__.ts`).
*   **Conditional File Generation:** Include/exclude template files or directories based on boolean variables.
*   **Pre/Post Generation Hooks:** Execute custom shell commands during the generation lifecycle.
*   **Cross-Platform:** Built with Rust for performance and easier cross-platform distribution.

## Goals

*   Provide a fast and reliable scaffolding experience.
*   Make template creation and maintenance significantly easier through integrated validation.
*   Offer flexibility in template design without relying on complex templating engines embedded in source files.
*   Become a go-to tool for teams needing to manage and distribute standardized project starting points.

## Current Status

Spawn Point is currently under active development. The core generation and validation mechanisms are implemented, but expect rough edges, API changes, and ongoing feature additions.

## Getting Started (Conceptual)

```bash
# List available templates
spawnpoint list

# Generate a project interactively
spawnpoint generate

# Generate a specific template
spawnpoint generate -l nodejs -t "Node.js Base v1" -o ./my-new-node-app

# Validate a template (use the exact name from scaffold.yaml)
spawnpoint validate rust "Rust CLI App v1"
```

## Creating Templates

1.  Create a directory under the `templates/` folder (e.g., `templates/my_cool_template`).
2.  Populate it with the files and directories for your base project. Use unique placeholder *values* (e.g., `__MY_VAR_PLACEHOLDER__`) where substitutions are needed.
3.  Create a `scaffold.yaml` manifest in the template's root directory, defining:
    *   `name`, `description`, `language`
    *   `variables` (mapping variable names to prompts, `placeholderValue` strings, optional `default`, `validation_regex`, etc.)
    *   Optional: `transformations`, `placeholderFilenames`, `conditionalPaths`, `preGenerate`, `postGenerate`
    *   Optional but highly recommended: A `validation` section with `testVariables` and `steps` (install, build, test commands). Define `env` maps within steps if specific environment variables are needed (otherwise the parent environment is inherited).
4.  Test validation: `spawnpoint validate <lang> "<Your Template Name>"`
5.  Test generation: `spawnpoint generate -l <lang> -t "<Your Template Name>"`

*(See existing templates for examples)*

## Contributing

Contributions are welcome! Please feel free to open issues or submit pull requests.

## License

Licensed under the **Mozilla Public License, v. 2.0** ([MPL-2.0](LICENSE) or https://opensource.org/licenses/MPL-2.0).