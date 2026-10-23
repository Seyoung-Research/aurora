//! The project data model: tables, analyses and graphs.
//!
//! A [`Project`] is the unit of persistence (see [`persistence`]). It bundles
//! the tables that hold the numbers, the analyses that were run on them and the
//! graphs that visualise results.
//!
//! Tables come in four [`TableFlavor`]s, distinguished by how their numbers are
//! organised:
//!
//! - [`TableFlavor::Xy`] — one `X` value with several `Y` replicates per row.
//! - [`TableFlavor::Column`] — one column of replicates per group.
//! - [`TableFlavor::Grouped`] — rows of X categories crossed with groups of
//!   replicates.
//! - [`TableFlavor::Contingency`] — counts for two categorical variables.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Import and export of CSV files for every table flavor.
pub mod csv_import;
/// Persistence: save / load a project as a JSON file.
pub mod persistence;
/// Concrete table types behind the [`TableFlavor`].
pub mod tables;

use crate::analysis::{Analysis, Graph};
use crate::plot::Chart;

/// Represents a generic data table in the system.
///
/// Implemented by the four table types ([`XyTable`](crate::data::tables::XyTable),
/// [`ColumnTable`](crate::data::tables::ColumnTable),
/// [`GroupedTable`](crate::data::tables::GroupedTable) and
/// [`ContingencyTable`](crate::data::tables::ContingencyTable)). The trait
/// exposes only the display name and the ability to clear the table's data.
pub trait Table {
    /// Display name of the table.
    fn name(&self) -> &str;
    /// Rename the table.
    fn set_name(&mut self, name: String);
    /// Remove all data, keeping the table itself.
    fn clear(&mut self);
}

/// Which data layout (table type) a table uses.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TableFlavor {
    /// X values with Y replicates.
    Xy,
    /// One column of replicates per group.
    Column,
    /// Rows (X categories) x groups x replicates.
    Grouped,
    /// Counts of row/column categories.
    Contingency,
}

impl TableFlavor {
    /// Short label used in menus, results and the project tree.
    pub fn label(&self) -> &'static str {
        match self {
            TableFlavor::Xy => "XY",
            TableFlavor::Column => "Column",
            TableFlavor::Grouped => "Grouped",
            TableFlavor::Contingency => "Contingency",
        }
    }

    /// All supported table flavors, in a stable display order.
    pub fn all() -> [TableFlavor; 4] {
        [
            TableFlavor::Xy,
            TableFlavor::Column,
            TableFlavor::Grouped,
            TableFlavor::Contingency,
        ]
    }
}

/// The master project structure containing all data and results.
///
/// A project is what gets saved to and loaded from a JSON file (see
/// [`persistence`]); it groups tables, the analyses derived from them and the
/// graphs that visualise results.
#[derive(Serialize, Deserialize, Default)]
pub struct Project {
    /// Display name of the project.
    pub name: String,
    /// Tables in the project, in creation order.
    pub tables: Vec<TableInstance>,
    /// Analyses run on (or planned for) the tables, in creation order.
    #[serde(default)]
    pub analyses: Vec<Analysis>,
    /// Graphs defined over the tables / analyses, in creation order.
    #[serde(default)]
    pub graphs: Vec<Graph>,
}

/// A table of one of the four [`TableFlavor`]s.
///
/// The enum exists so the rest of the engine can be written over a single
/// interface (name, kind, data access) while still keeping the concrete type for
/// each flavour.
#[derive(Serialize, Deserialize)]
pub enum TableInstance {
    /// X values with Y replicates.
    Xy(tables::XyTable),
    /// One column of replicates per group.
    Column(tables::ColumnTable),
    /// Rows (X categories) x groups x replicates.
    Grouped(tables::GroupedTable),
    /// Counts of row/column categories.
    Contingency(tables::ContingencyTable),
}

impl TableInstance {
    /// Read-only access to the underlying table, whatever its flavour is.
    pub fn as_table(&self) -> &dyn Table {
        match self {
            TableInstance::Xy(t) => t,
            TableInstance::Column(t) => t,
            TableInstance::Grouped(t) => t,
            TableInstance::Contingency(t) => t,
        }
    }

    /// Mutable access to the underlying table, whatever its flavour is.
    pub fn as_table_mut(&mut self) -> &mut dyn Table {
        match self {
            TableInstance::Xy(t) => t,
            TableInstance::Column(t) => t,
            TableInstance::Grouped(t) => t,
            TableInstance::Contingency(t) => t,
        }
    }

    /// Which flavour of table this value is.
    pub fn flavor(&self) -> TableFlavor {
        match self {
            TableInstance::Xy(_) => TableFlavor::Xy,
            TableInstance::Column(_) => TableFlavor::Column,
            TableInstance::Grouped(_) => TableFlavor::Grouped,
            TableInstance::Contingency(_) => TableFlavor::Contingency,
        }
    }
}

impl Project {
    /// Create a new, empty project with the given display name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tables: Vec::new(),
            analyses: Vec::new(),
            graphs: Vec::new(),
        }
    }

    /// Add a table and return its index in [`Project::tables`].
    pub fn add_table(&mut self, table: TableInstance) -> usize {
        self.tables.push(table);
        self.tables.len() - 1
    }

    /// Add an analysis and return its index in [`Project::analyses`].
    pub fn add_analysis(&mut self, analysis: Analysis) -> usize {
        self.analyses.push(analysis);
        self.analyses.len() - 1
    }

    /// Add a graph and return its index in [`Project::graphs`].
    pub fn add_graph(&mut self, graph: Graph) -> usize {
        self.graphs.push(graph);
        self.graphs.len() - 1
    }

    /// Names of all tables, in creation order.
    pub fn table_names(&self) -> Vec<String> {
        self.tables.iter().map(|t| t.as_table().name().to_string()).collect()
    }

    /// Re-run one analysis against its table, storing the result or the error.
    ///
    /// The error, if any, is recorded on the analysis (`Analysis::error`) so the
    /// results panel can show why a run failed; the previous result is cleared.
    pub fn run_analysis(&mut self, index: usize) -> Result<()> {
        let analysis = self
            .analyses
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("No such analysis"))?;
        let table_index = analysis.table;
        let table = self
            .tables
            .get(table_index)
            .ok_or_else(|| anyhow::anyhow!("Analysis references a missing table"))?;
        // Borrow-checker friendly: run against a clone-free split borrow.
        let spec = self.analyses[index].spec.clone();
        let table_flavor_ok = spec.applicable_to(table.flavor());
        let result = if table_flavor_ok {
            crate::analysis::run(&spec, table)
        } else {
            Err(anyhow::anyhow!(
                "{} cannot be run on a {} table",
                spec.label(),
                table.flavor().label()
            ))
        };
        let a = &mut self.analyses[index];
        match result {
            Ok(r) => {
                a.result = Some(r);
                a.error = None;
            }
            Err(e) => {
                a.result = None;
                a.error = Some(e.to_string());
            }
        }
        Ok(())
    }

    /// Run every analysis in the project, storing a result or an error each.
    pub fn run_all_analyses(&mut self) {
        for i in 0..self.analyses.len() {
            let _ = self.run_analysis(i);
        }
    }

    /// Chart for one graph, if its table still exists.
    pub fn chart_for_graph(&self, index: usize) -> Option<Chart> {
        let graph = self.graphs.get(index)?;
        crate::analysis::build_chart(graph, &self.tables, &self.analyses)
    }

    /// Indices of the analyses that target the given table.
    pub fn analyses_for_table(&self, table: usize) -> Vec<usize> {
        self.analyses
            .iter()
            .enumerate()
            .filter(|(_, a)| a.table == table)
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::tables::*;
    use super::*;
    use crate::analysis::{AnalysisSpec, Graph};
    use crate::plot::{SeriesStyle, ErrorKind};

    fn sample_project() -> Project {
        let mut p = Project::new("Test");
        p.add_table(TableInstance::Column(ColumnTable {
            name: "Groups".into(),
            columns: vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]],
            group_names: vec!["A".into(), "B".into()],
        }));
        p.add_table(TableInstance::Xy(XyTable {
            name: "XY".into(),
            data: vec![(1.0, vec![1.0, 1.1]), (2.0, vec![2.0, 2.1])],
        }));
        p
    }

    #[test]
    fn flavors_and_names() {
        let p = sample_project();
        assert_eq!(p.tables[0].flavor(), TableFlavor::Column);
        assert_eq!(p.tables[1].flavor(), TableFlavor::Xy);
        assert_eq!(p.table_names(), vec!["Groups".to_string(), "XY".to_string()]);
        assert!(TableFlavor::Column.label().contains("Column"));
    }

    #[test]
    fn run_analysis_stores_result_or_error() {
        let mut p = sample_project();
        let idx = p.add_analysis(Analysis::new(
            AnalysisSpec::TTest {
                kind: crate::stats::comparison::TTestKind::Unpaired,
                group_a: 0,
                group_b: 1,
            },
            0,
        ));
        assert!(p.run_analysis(idx).is_ok());
        assert!(p.analyses[idx].result.is_some());
        assert!(p.analyses[idx].error.is_none());

        // Wrong table type -> error recorded, no panic.
        let bad = p.add_analysis(Analysis::new(AnalysisSpec::LinearRegression, 0));
        assert!(p.run_analysis(bad).is_ok());
        assert!(p.analyses[bad].result.is_none());
        assert!(p.analyses[bad].error.is_some());
    }

    #[test]
    fn chart_for_graph_uses_table() {
        let mut p = sample_project();
        let g = p.add_graph(Graph {
            name: "Columns".into(),
            table: Some(0),
            style: SeriesStyle::Bars,
            error: ErrorKind::Sem,
            ..Default::default()
        });
        let chart = p.chart_for_graph(g).unwrap();
        assert_eq!(chart.series.len(), 2);
        assert_eq!(chart.categories, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn analyses_for_table_filters() {
        let mut p = sample_project();
        p.add_analysis(Analysis::new(AnalysisSpec::OneWayAnova, 0));
        p.add_analysis(Analysis::new(AnalysisSpec::LinearRegression, 1));
        assert_eq!(p.analyses_for_table(0), vec![0]);
        assert_eq!(p.analyses_for_table(1), vec![1]);
    }
}
