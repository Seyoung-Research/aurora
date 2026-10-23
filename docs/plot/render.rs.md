# Raster Renderer (`src/plot/render.rs`)

The `render` module is responsible for taking a `Chart` model and drawing it into a pixel buffer. It is built on top of `plotters` but implements its own axis and layout logic for maximum control over scientific plotting requirements.

## Rendering Pipeline

### 1. Layout Calculation
Before drawing, the renderer computes a `Layout` object. This determines:
- The exact pixel dimensions of the plotting area (after accounting for margins/padding).
- The data-space ranges for the X and Y axes.
- The positions of the "nice" ticks in screen coordinates.

### 2. Drawing Primitives
The renderer uses low-level drawing helpers to render the chart components:
- **Axes and Grids**: Draws the frame, axis lines, and grid lines.
- **Series**: Depending on the `SeriesStyle`, it calls specific drawing functions:
    - `draw_line_series`: Connects points with lines.
    - `draw_markers`: Renders marker shapes (Circle, Square, Diamond, etc.).
    - `draw_errors`: Renders vertical error bars.
    - `draw_bars` / `draw_columns`: Renders rectangles for bar charts.
    - `draw_boxes`: Renders Tukey-style box plots (whisker, box, median line).
- **Legend**: Renders a color-coded key for the different series.

### 3. Text Rendering
Labels and ticks are rendered using a fallback system. If the requested system font is unavailable, the renderer logs the failure but continues drawing to ensure the application doesn't crash.

## Output Formats
The renderer provides two primary output paths:
- **`render(...)`**: Produces a `ChartImage` (RGB byte buffer) used by the GUI to create an `egui::Texture`.
- **`export_png(...)`**: Produces a fully encoded PNG byte stream for file export.
