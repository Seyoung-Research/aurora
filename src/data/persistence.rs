//! The `persistence` module handles the serialization and deserialization of the `Project` state to the local filesystem.
//! ## Implementation
//! Aurora uses `serde_json` for persistence. The entire state of a project—including all tables, analyses, and graphs—is saved into a single, human-readable JSON file.
//! ### Key Functions
//! - **`save_project(project, path)`**: Serializes the `Project` struct to a pretty-printed JSON file using a `BufWriter`.
//! - **`load_project(path)`**: Reads a JSON file from disk and deserializes it back into a `Project` instance using a `BufReader`.


use std::fs::File;
use std::io::{BufReader, BufWriter};
use anyhow::Result;
use crate::data::Project;

/// Serialize a project to a pretty-printed JSON file.
///
/// The whole project — tables, analyses and graphs — is written as one JSON
/// document, using a buffered writer.
///
/// # Errors
///
/// Returns an error if the file cannot be created or if serialisation fails.
pub fn save_project(project: &Project, path: &str) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, project)?;
    Ok(())
}

/// Read a project back from a JSON file.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or if it is not a valid
/// project file.
pub fn load_project(path: &str) -> Result<Project> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let project = serde_json::from_reader(reader)?;
    Ok(project)
}
