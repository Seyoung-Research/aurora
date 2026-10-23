# Project Data Model (`src/data/mod.rs`)

The `data` module defines the core data structures of Aurora. A `Project` is the master container that encapsulates all experimental data and its derived results.

## Project Structure
The `Project` struct is designed for total persistence (JSON) and contains:
- **`tables`**: A collection of `TableInstance` enums.
- **`analyses`**: A list of statistical tests performed on those tables.
- **`graphs`**: A list of visual configurations.

## Table Abstractions
Aurora uses a trait-based approach to handle different data layouts via the `Table` trait.

### Table Flavors (`TableFlavor`)
Different scientific data layouts require different internal structures:
- **XY**:- X values with associated Y replicates.
- **Column**:- One column of replicates per group.
- **Grouped**:- A 3D structure: `[row labels][groups][replicates]`.
- **Contingency**:- A 2D grid of integer frequency counts.

### TableInstance
`TableInstance` is an enum that wraps the specific table types (`XyTable`, `ColumnTable`, etc.), allowing them to be stored in a single `Vec` within the `Project` while maintaining type safety via pattern matching.
