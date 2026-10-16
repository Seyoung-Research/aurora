use std::fs::File;
use std::io::{BufReader, BufWriter};
use anyhow::Result;
use crate::data::Project;

pub fn save_project(project: &Project, path: &str) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, project)?;
    Ok(())
}

pub fn load_project(path: &str) -> Result<Project> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let project = serde_json::from_reader(reader)?;
    Ok(project)
}
