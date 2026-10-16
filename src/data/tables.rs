use serde::{Serialize, Deserialize};
use crate::data::Table;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct XyTable {
    pub name: String,
    /// Each element is (X, Y_replicates)
    pub data: Vec<(f64, Vec<f64>)>,
}

impl Table for XyTable {
    fn name(&self) -> &str { &self.name }
    fn set_name(&mut self, name: String) { self.name = name; }
    fn clear(&mut self) { self.data.clear(); }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ColumnTable {
    pub name: String,
    /// Each inner Vec is a column of replicates for a group
    pub columns: Vec<Vec<f64>>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GroupedTable {
    pub name: String,
    /// Row labels (X-axis)
    pub row_labels: Vec<String>,
    /// Data organized as [row][group][replicate]
    pub data: Vec<Vec<Vec<f64>>>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContingencyTable {
    pub name: String,
    pub row_labels: Vec<String>,
    pub col_labels: Vec<String>,
    /// Frequency counts
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
