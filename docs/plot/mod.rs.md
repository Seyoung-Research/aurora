# Plotting and Chart Model (`src/plot/mod.rs`)

The `plot` module defines the intermediate `Chart` model and the high-level builders that transform raw data into a renderable state.

## The Chart Model

A `Chart` is a self-describing object that contains all the information needed by the renderer:
- **`Series`**: A collection of `DataPoint`s, each with a value, optional replicates, and a specific `SeriesStyle` (e.g., Bars, Lines).
- **`PlotKind`**: Determines the layout of the X-axis (XY, Categorical, or Grouped).
- **`Axis`**: Configuration for labels, scales (Linear/Log), and tick counts.

## Chart Builders

To simplify the creation of charts from tables, the module provides high-level builders:
- **`xy_chart`**: Creates a chart from an `XyTable`. It can either plot the mean of replicates with error bars or plot every replicate as an individual jittered point.
- **`column_chart`**: Creates a chart from a `ColumnTable` (e.g., for bar charts or box plots).
- **`grouped_chart`**: Creates a chart from a `GroupedTable`, where each group gets its own series across multiple X-axis categories.

## Visual Styles

### SeriesStyle
Defines how data is visualized:
- `LinesAndPoints`: Standard XY plot.
- `Bars`: Bar charts.
- `Boxes`: Tukey-style box plots (Min, Q1, Median, Q3, Max).
- `Columns`: Jittered points or bars.

### ErrorKind
Determines how error bars are derived from replicates:
- `SEM`: Standard Error of the Mean.
- `SD`: Standard Deviation.
- `None`: No error bars.
