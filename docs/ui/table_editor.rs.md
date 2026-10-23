# Table Editor (`src/ui/table_editor.rs`)

The Table Editor provides a spreadsheet-like interface for editing experimental data.

## Dispatch Pattern

Since Aurora supports multiple data layouts, the editor uses a dispatcher pattern:
```rust
pub fn table_editor(ui: &mut Ui, table: &mut TableInstance) -> bool {
    match table {
        TableInstance::Xy(t) => xy_editor(ui, t),
        TableInstance::Column(t) => column_editor(ui, t),
        TableInstance::Grouped(t) => grouped_editor(ui, t),
        TableInstance::Contingency(t) => contingency_editor(ui, t),
    }
}
```

## Implementation Details

- **Grid-based Layout**: Uses `egui::Grid` to align cells.
- **Dynamic Sizing**: Provides buttons to add/remove rows and groups (columns) on the fly.
- **Type-specific Editors**:
    - `column_editor`: Manages a list of columns (groups) and rows of values.
    - `xy_editor`: Specifically handles X-Y coordinate pairs.
    - `grouped_editor`: Handles replicates within groups.
    - `contingency_editor`: Handles count-based frequency tables.
