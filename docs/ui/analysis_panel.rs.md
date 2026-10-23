# Analysis Panel (`src/ui/analysis_panel.rs`)

The Analysis Panel is where statistical tests are configured and their results interpreted.

## Deferred Action Pattern

Because modifying the `analyses` vector while iterating over it for rendering is forbidden by Rust's borrowing rules, the panel uses a **Deferred Action** pattern:
1. **UI Pass**: The panel renders the list of analyses. If a user clicks "Delete" or "Add", an `Action` is pushed to a local queue.
2. **Application Pass**: After the UI rendering, the panel iterates through the `actions` queue and applies the mutations to the `Project` state.

## Features

- **Analysis Configuration**: Provides specific widgets based on the test type (e.g., a `ComboBox` for `TTestKind` or a `Slider` for Lowess smoothing fraction).
- **Group Picking**: A `group_picker` helper allows users to select which groups from the table are used as "Group A" and "Group B" for comparisons.
- **Result Rendering**: Renders a summary table of statistical results (p-values, means, etc.) using a striped `egui::Grid`.
- **Graph Integration**: Allows one-click creation of a graph based on the results of the selected analysis.
