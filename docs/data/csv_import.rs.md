# CSV Import and Export (`src/data/csv_import.rs`)

The `csv_import` module provides tools to bridge external CSV files with Aurora's internal `TableInstance` types.

## Import Formats

The module implements four specific import strategies based on the `TableFlavor`:

| Flavor | CSV Layout | Description |
|---|---|---|
| **XY** | `X, Y1, Y2, ...` | One row per X value; subsequent columns are replicates. |
| **Column** | `Group A, Group B, ...` | One column per group; each row is a subject. |
| **Grouped** | `, A, A, B, B` | Row label followed by group names repeated for each replicate. |
| **Contingency**| `, Cat 1, Cat 2` | Row label followed by integer counts. |

## Implementation Details
- **Parsing**: Uses the `csv` crate for robust parsing.
- **Missing Values**: The importer is designed to skip empty cells, treating them as missing values rather than zero.
- **Symmetry**: The `export_table_csv` function ensures that any table exported from Aurora can be imported back perfectly, maintaining a symmetric round-trip.
