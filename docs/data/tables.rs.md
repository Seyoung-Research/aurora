# Table Implementations (`src/data/tables.rs`)

This module implements the concrete data structures for the different table flavors supported by Aurora. All table types implement the `Table` trait, providing a common interface for naming and clearing data.

## Table Types

### XyTable
Used for scatter plots and curve fitting.
- **Structure**: `Vec<(f64, Vec<f64>)>`.
- **Logic**: Each entry consists of a single X value and a list of Y replicates measured at that X.

### ColumnTable
The most common format for bar charts and box plots.
- **Structure**: `Vec<Vec<f64>>` for data and `Vec<String>` for group names.
- **Logic**: Each inner vector is a "column" representing all replicates for one experimental group.

### GroupedTable
Used for two-way ANOVA or grouped bar charts.
- **Structure**: `Vec<Vec<Vec<f64>>>`.
- **Logic**: Data is organized as `[row][group][replicate]`. This allows for multiple X-axis categories (rows), each containing multiple groups, each with multiple replicates.

### ContingencyTable
Used for Chi-square tests of independence.
- **Structure**: `Vec<Vec<u64>>`.
- **Logic**: A frequency matrix where each cell contains an unsigned integer representing the count of observations for that row/column category pair.
