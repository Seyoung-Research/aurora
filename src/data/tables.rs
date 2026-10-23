//! Table types: the concrete shapes behind each table flavor.
//!
//! Each type implements the [`Table`] trait (name + clear) and carries the
//! numbers in its own layout.

use serde::{Serialize, Deserialize};
use crate::data::Table;

/// An XY table: a set of measured points, each `X` with several `Y`
/// replicates.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct XyTable {
    /// Display name of the table.
    pub name: String,
    /// Each element is (X, Y_replicates)
    pub data: Vec<(f64, Vec<f64>)>,
}

impl Table for XyTable {
    fn name(&self) -> &str { &self.name }
    fn set_name(&mut self, name: String) { self.name = name; }
    fn clear(&mut self) { self.data.clear(); }
}

/// A column table: one column of replicates per group, i.e. group-by-group
/// measurements (one row per subject within a group).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ColumnTable {
    /// Display name of the table.
    pub name: String,
    /// Each inner Vec is a column of replicates for a group
    pub columns: Vec<Vec<f64>>,
    /// Names of the groups (columns), parallel to `columns`.
    pub group_names: Vec<String>,
}

impl Table for ColumnTable {
    fn name(&self) -> &str { &self.name }
    fn set_name(&mut self, name: String) { self.name = name; }
    fn clear(&mut self) { 
        self.columns.clear(); 
        self.group_names.clear(); 
    }
}

/// A grouped table: rows of X categories crossed with groups of replicates.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GroupedTable {
    /// Display name of the table.
    pub name: String,
    /// Row labels (X-axis)
    pub row_labels: Vec<String>,
    /// Data organized as rows × groups × replicates.
    pub data: Vec<Vec<Vec<f64>>>,
    /// Names of the groups.
    pub group_names: Vec<String>,
}

impl Table for GroupedTable {
    fn name(&self) -> &str { &self.name }
    fn set_name(&mut self, name: String) { self.name = name; }
    fn clear(&mut self) {
        self.row_labels.clear();
        self.data.clear();
        self.group_names.clear();
    }
}

/// A contingency (two-way) table of counts.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContingencyTable {
    /// Display name of the table.
    pub name: String,
    /// Row labels (the first categorical variable).
    pub row_labels: Vec<String>,
    /// Column labels (the second categorical variable).
    pub col_labels: Vec<String>,
    /// Frequency counts, one row per row-label combination.
    pub data: Vec<Vec<u64>>,
}

impl Table for ContingencyTable {
    fn name(&self) -> &str { &self.name }
    fn set_name(&mut self, name: String) { self.name = name; }
    fn clear(&mut self) {
        self.row_labels.clear();
        self.col_labels.clear();
        self.data.clear();
    }
}
