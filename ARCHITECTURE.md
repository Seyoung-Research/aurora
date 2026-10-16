# System Architecture: Aurora

## 1. Module Decomposition

### `aurora::data`
- **`Table` Trait**: Interface for all table types.
- **`XyTable`, `ColumnTable`, `GroupedTable`, `ContingencyTable`**: Structs implementing `Table`.
- **`Project`**: Root container for multiple tables and analysis results.
- **`Persistence`**: Logic for saving/loading projects (Serde $\rightarrow$ JSON/Bincode).

### `aurora::stats`
- **`Descriptive`**: Module for basic stats.
- **`Comparison`**: Module for T-tests and ANOVA.
- **`Regression`**: 
    - `Linear`: OLS, Deming.
    - `Nonlinear`: Wrapper around `argmin` for Levenberg-Marquardt optimization.
    - `Models`: Pre-defined model functions (e.g., `sigmoidal_dose_response`).
- **`CurveFitting`**: Splines, LOWESS, Integration.

### `aurora::plot`
- **`Chart`**: High-level representation of a plot.
- **`Axis`**: Configuration for scales, labels, and limits.
- **`PlotElement`**: Enum for Points, Lines, Bars, ErrorBars.
- **`Backend`**: Adapter for the rendering library (e.g., Plotters or Vello).

### `aurora::ui`
- **`App`**: Main `egui` application state.
- **`TableEditor`**: Grid-based data entry interface.
- **`AnalysisPanel`**: Configuration UI for stats.
- **`GraphViewer`**: Interactive plot display and customization.

## 2. Data Flow
`User Input` $\rightarrow$ `Project.Tables` $\rightarrow$ `stats::Analysis` $\rightarrow$ `AnalysisResult` $\rightarrow$ `plot::Chart` $\rightarrow$ `Rendering Backend` $\rightarrow$ `Screen`.

## 3. Decision Log: GUI & Rendering
- **GUI**: `egui`. 
    - *Why*: Immediate mode is ideal for scientific tools where state changes frequently (sliders for axis limits, toggles for error bars). Extremely portable.
- **Rendering**: `plotters` (for initial version).
    - *Why*: Mature, supports multiple backends (Bitmap, SVG), and handles the complex logic of scientific axes well.
- **Math**: `ndarray` + `argmin`.
    - *Why*: Standard Rust ecosystem for numerical computing.
