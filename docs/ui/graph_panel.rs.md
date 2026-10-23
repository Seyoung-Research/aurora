# Graph Panel (`src/ui/graph_panel.rs`)

The Graph Panel converts numerical data and statistical results into visual charts.

## Texture Caching (`TexCache`)

Rendering high-quality charts is computationally expensive. To maintain 60 FPS, the panel uses a `TexCache`:
- It generates a unique hash based on the graph's options and the current project `revision`.
- If the hash matches a cached entry, the panel simply displays the existing `egui::TextureHandle`.
- If the `revision` has changed or options were modified, the chart is re-rendered to a new texture.

## Visualization Options

The panel provides an extensive set of controls for the selected graph:
- **Axes**: Toggle log scales for X and Y axes.
- **Labels**: Editable strings for the title, X-axis, and Y-axis.
- **Styling**: Selection of `SeriesStyle` (e.g., Lines and Points vs. Bars) and `ErrorKind` (e.g., SEM, SD).
- **Regression**: Toggle for showing fit lines (Linear, Lowess, etc.).

## Export Workflow

PNG exports are handled via a request-response loop:
1. `GraphPanel` fills an `ExportRequest` struct and passes it back to `App`.
2. `App` opens an "Export as PNG" dialog for the user to specify the filename and dimensions.
3. The final render is performed by `crate::plot::render::save_png`.
