# Application Entry Point (`src/main.rs`)

`main.rs` serves as the entry point for the Aurora application, coordinating the startup of the GUI and providing a headless mode for verification.

## Execution Modes

### 1. Desktop Application (GUI)
When run without specific arguments, the application initializes `eframe` and launches the `ui::App` as a native window.

### 2. Headless Self-Test (`--selftest`)
Aurora includes a comprehensive self-test mode used for CI/CD and headless verification. When the `--selftest` flag is provided:
- It builds a `demo_project`.
- It executes every possible statistical analysis for every table in that project.
- It renders every generated graph.
- If the `--write-samples <dir>` flag is provided (or the `GP_WRITE_SAMPLES` environment variable is set), it exports the resulting charts as PNG files.
- The test fails (exits non-zero) if any step in the pipeline errors.

## Core Orchestration
The `main` function handles basic CLI flags (`-h`, `--help`), initializes the `env_logger` for system logging, and hands over execution to the `eframe` runtime.
