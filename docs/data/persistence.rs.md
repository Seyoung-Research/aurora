# Project Persistence (`src/data/persistence.rs`)

The `persistence` module handles the serialization and deserialization of the `Project` state to the local filesystem.

## Implementation
Aurora uses `serde_json` for persistence. The entire state of a project—including all tables, analyses, and graphs—is saved into a single, human-readable JSON file.

### Key Functions
- **`save_project(project, path)`**: Serializes the `Project` struct to a pretty-printed JSON file using a `BufWriter`.
- **`load_project(path)`**: Reads a JSON file from disk and deserializes it back into a `Project` instance using a `BufReader`.
