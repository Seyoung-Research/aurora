# Analysis and Graph Modeling (`src/analysis.rs`)

The `analysis.rs` module defines the logic for what statistical computations are performed, how their results are stored, and how that data is transformed into a visual `Chart` model.

## Statistical Analysis Workflow

The analysis system follows a three-step lifecycle: **Spec $\rightarrow$ Result $\rightarrow$ Chart**.

### 1. Analysis Specification (`AnalysisSpec`)
`AnalysisSpec` is an enum defining the *intent* of an analysis. It contains the parameters required to run a test, such as:
- **T-Tests**: Which groups to compare and whether to use Welch's correction.
- **Curve Fitting**: Which mathematical model (e.g., Sigmoidal, Exponential) to use.
- **Smoothing**: The fraction of points to use for LOWESS.

### 2. Analysis Results (`AnalysisResult`)
The `run` function takes an `AnalysisSpec` and a `TableInstance` to produce an `AnalysisResult`. Results are stored as:
- **NamedStats**: A map of metric names to their computed values (e.g., `"p-value"` $\rightarrow$ `0.042`).
- **TTestResult / AnovaResult**: Specialized structs for hypothesis tests containing statistics, degrees of freedom, and p-values.

### 3. Graph Modeling (`Graph`)
A `Graph` is a persistent description of a visualization. It references a table (and optionally an analysis) and specifies:
- **PlotKind**: The layout type (XY, Bar, Grouped Bar).
- **SeriesStyle**: How data is drawn (Lines, Points, Bars, Boxes).
- **ErrorKind**: Which error bars to calculate (SEM, SD).
- **Aesthetics**: Labels, titles, and log-scale toggles.

The `build_chart` function transforms a `Graph` definition into a `Chart` object, which is the final, render-ready representation of the data.

## Key Functions
- `run(...)`: The dispatcher that executes the specific statistical logic based on the `AnalysisSpec`.
- `build_chart(...)`: Maps the high-level `Graph` model to the low-level `Chart` primitives used by the rendering engine.
- `table_group_count(...)`: Helper to determine how many groups a table exposes, used by the UI to populate the "Add Analysis" menu.
