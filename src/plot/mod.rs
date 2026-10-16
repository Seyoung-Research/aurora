//! Plotting: Aurora's chart model plus a pixel renderer.
//!
//! The renderer draws into an RGB byte buffer with [`plotters`], which the GUI
//! converts into an egui texture and which can be exported as PNG.

pub mod png;
pub mod render;
pub mod scale;

use serde::{Deserialize, Serialize};

pub use scale::{Axis, ScaleKind};

use crate::stats::descriptive::calculate_descriptive;

/// Marker shapes for scatter-type series.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    None,
    Circle,
    Square,
    Triangle,
    InvertedTriangle,
    Diamond,
    Cross,
    Plus,
}

impl Default for Marker {
    fn default() -> Self {
        Marker::Circle
    }
}

/// How a series is drawn.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeriesStyle {
    /// Markers only.
    Points,
    /// Connected line only.
    Lines,
    /// Connected line with markers.
    LinesAndPoints,
    /// Filled bars at the categorical positions.
    Bars,
    /// Box-and-whisker computed from the raw values of each point.
    Boxes,
    /// Every raw value drawn as a jittered marker (a "scatter" column).
    Columns,
    /// Straight bars from the baseline with the mean marked.
    MeanBars,
}

impl Default for SeriesStyle {
    fn default() -> Self {
        SeriesStyle::LinesAndPoints
    }
}

/// The kind of error bar to derive from replicates.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    None,
    Sd,
    Sem,
    Ci95,
    Range,
}

impl Default for ErrorKind {
    fn default() -> Self {
        ErrorKind::Sem
    }
}

impl ErrorKind {
    pub fn label(&self) -> &'static str {
        match self {
            ErrorKind::None => "None",
            ErrorKind::Sd => "SD",
            ErrorKind::Sem => "SEM",
            ErrorKind::Ci95 => "95% CI",
            ErrorKind::Range => "Range",
        }
    }

    pub fn all() -> [ErrorKind; 5] {
        [
            ErrorKind::None,
            ErrorKind::Sd,
            ErrorKind::Sem,
            ErrorKind::Ci95,
            ErrorKind::Range,
        ]
    }
}

/// One plotted point. `raw` optionally carries the underlying replicates so
/// box plots and jittered columns can be drawn from it.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    /// Symmetric error bar half-height.
    pub y_err: Option<f64>,
    /// Asymmetric lower error bar (drawn when present).
    pub y_err_low: Option<f64>,
    /// Category label (categorical plots).
    pub label: Option<String>,
    /// Raw replicate values.
    pub raw: Vec<f64>,
}

impl DataPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            ..Default::default()
        }
    }

    pub fn with_err(x: f64, y: f64, err: f64) -> Self {
        Self {
            x,
            y,
            y_err: Some(err),
            ..Default::default()
        }
    }

    pub fn with_raw(x: f64, y: f64, raw: Vec<f64>) -> Self {
        Self {
            x,
            y,
            raw,
            ..Default::default()
        }
    }

    /// Lower/upper extents of the error bar relative to `y`.
    pub fn err_extents(&self) -> Option<(f64, f64)> {
        let up = self.y_err?;
        let down = self.y_err_low.unwrap_or(up);
        Some((self.y - down, self.y + up))
    }
}

/// A data series (one dataset in a graph).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub name: String,
    pub style: SeriesStyle,
    pub color: [u8; 3],
    pub marker: Marker,
    pub points: Vec<DataPoint>,
    pub line_width: u32,
    pub marker_size: u32,
    /// Fill opacity in percent for bars/areas (0-100).
    pub fill_alpha: u8,
}

impl Series {
    pub fn new(name: impl Into<String>, style: SeriesStyle, color: [u8; 3]) -> Self {
        Self {
            name: name.into(),
            style,
            color,
            marker: Marker::Circle,
            points: Vec::new(),
            line_width: 2,
            marker_size: 4,
            fill_alpha: 100,
        }
    }

    pub fn with_points(mut self, points: Vec<DataPoint>) -> Self {
        self.points = points;
        self
    }

    pub fn with_marker(mut self, marker: Marker) -> Self {
        self.marker = marker;
        self
    }

    pub fn with_fill_alpha(mut self, alpha: u8) -> Self {
        self.fill_alpha = alpha.min(100);
        self
    }

    /// Points that can actually be drawn on the given scales.
    fn drawable(&self, xs: ScaleKind, ys: ScaleKind) -> Vec<(f64, f64)> {
        self.points
            .iter()
            .filter_map(|p| {
                let x = scale::transform(p.x, xs)?;
                let y = scale::transform(p.y, ys)?;
                Some((x, y))
            })
            .collect()
    }
}

/// The overall graph type, which drives the layout of the x axis.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotKind {
    /// Numeric x axis.
    Xy,
    /// Categorical x axis with one position per category.
    Column,
    /// One box per categorical position.
    BoxAndWhisker,
    /// Bars (mean +/- error) at categorical positions.
    Bar,
    /// Bars grouped by category, with one bar per series per category.
    GroupedBar,
}

impl Default for PlotKind {
    fn default() -> Self {
        PlotKind::Xy
    }
}

/// A complete, self-describing graph ready to be rendered.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chart {
    pub title: String,
    pub x_axis: Axis,
    pub y_axis: Axis,
    pub kind: PlotKind,
    pub series: Vec<Series>,
    pub legend: bool,
    /// Horizontal jitter applied to raw points in `Columns` plots (0.0-0.5).
    pub jitter: f64,
    /// Tick labels for categorical plots.
    pub categories: Vec<String>,
    pub frame: bool,
    /// Draw light horizontal grid lines.
    pub grid: bool,
    /// Fill colour of the whole canvas.
    pub background: [u8; 3],
    pub width: u32,
    pub height: u32,
}

impl Default for Chart {
    fn default() -> Self {
        Self {
            title: String::new(),
            x_axis: Axis::default(),
            y_axis: Axis::default(),
            kind: PlotKind::Xy,
            series: Vec::new(),
            legend: true,
            jitter: 0.25,
            categories: Vec::new(),
            frame: true,
            grid: true,
            background: [255, 255, 255],
            width: 800,
            height: 600,
        }
    }
}

impl Chart {
    pub fn new(title: impl Into<String>, kind: PlotKind) -> Self {
        Self {
            title: title.into(),
            kind,
            ..Default::default()
        }
    }

    pub fn with_axes(mut self, x: Axis, y: Axis) -> Self {
        self.x_axis = x;
        self.y_axis = y;
        self
    }

    pub fn with_series(mut self, s: Series) -> Self {
        self.series.push(s);
        self
    }

    pub fn legend(mut self, on: bool) -> Self {
        self.legend = on;
        self
    }

    /// Data range of the x values currently present (drawing space).
    pub fn x_data_range(&self) -> Option<(f64, f64)> {
        self.data_range(true)
    }

    /// Data range of the y values currently present (drawing space, so error
    /// bars and box plots are included).
    pub fn y_data_range(&self) -> Option<(f64, f64)> {
        self.data_range(false)
    }

    fn data_range(&self, is_x: bool) -> Option<(f64, f64)> {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut acc = |v: f64| {
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        };
        for s in &self.series {
            for p in &s.points {
                if is_x {
                    acc(p.x);
                    if s.style == SeriesStyle::Bars
                        || s.style == SeriesStyle::MeanBars
                        || s.style == SeriesStyle::Boxes
                    {
                        acc(p.x - 0.5);
                        acc(p.x + 0.5);
                    } else if s.style == SeriesStyle::Columns {
                        acc(p.x - self.jitter);
                        acc(p.x + self.jitter);
                    }
                } else {
                    // value extent
                    if s.style == SeriesStyle::Boxes
                        || s.style == SeriesStyle::Columns
                    {
                        for &v in &p.raw {
                            acc(v);
                        }
                    } else {
                        acc(p.y);
                        if let Some((l, u)) = p.err_extents() {
                            acc(l);
                            acc(u);
                        }
                    }
                }
            }
        }
        if lo.is_finite() && hi.is_finite() {
            Some((lo, hi))
        } else {
            None
        }
    }

    /// Bars should include a baseline at zero.
    pub fn needs_zero_baseline(&self) -> bool {
        matches!(
            self.kind,
            PlotKind::Bar | PlotKind::GroupedBar
        ) || self
            .series
            .iter()
            .any(|s| matches!(s.style, SeriesStyle::Bars | SeriesStyle::MeanBars))
    }
}

/// The default colour palette for successive datasets.
pub fn palette_color(i: usize) -> [u8; 3] {
    const PALETTE: [[u8; 3]; 12] = [
        [0, 114, 178],   // blue
        [213, 94, 0],    // vermillion
        [0, 158, 115],   // bluish green
        [204, 121, 167], // reddish purple
        [86, 180, 233],  // sky blue
        [230, 159, 0],   // orange
        [240, 228, 66],  // yellow
        [0, 0, 0],       // black
        [187, 187, 187], // grey
        [153, 51, 51],   // brick red
        [51, 102, 153],  // steel
        [102, 153, 51],  // olive
    ];
    PALETTE[i % PALETTE.len()]
}

// ---------------------------------------------------------------------------
// Chart builders
// ---------------------------------------------------------------------------

/// Summary of a replicate column, used by the builders below.
pub fn summarise(values: &[f64], err: ErrorKind) -> Option<(f64, Option<f64>, Option<f64>)> {
    if values.is_empty() {
        return None;
    }
    let s = calculate_descriptive(values)?;
    let (up, low) = match err {
        ErrorKind::None => (None, None),
        ErrorKind::Sd => (Some(s.std_dev), None),
        ErrorKind::Sem => (Some(s.std_error), None),
        ErrorKind::Ci95 => (
            Some(s.ci95_upper - s.mean),
            Some(s.mean - s.ci95_lower),
        ),
        ErrorKind::Range => (Some(s.max - s.mean), Some(s.mean - s.min)),
    };
    Some((s.mean, up, low))
}

/// One series with the mean of each X row's replicates, plus error bars.
///
/// `data` is the column-oriented layout of `XyTable`: one `(x, y_replicates)`
/// entry per row. When `style` is [`SeriesStyle::Columns`] every replicate is
/// drawn as an individual (jittered) point instead of a mean.
pub fn xy_chart(
    title: impl Into<String>,
    x_label: impl Into<String>,
    y_label: impl Into<String>,
    data: &[(f64, Vec<f64>)],
    err: ErrorKind,
    style: SeriesStyle,
    x_log: bool,
    y_log: bool,
) -> Chart {
    let mut chart = Chart::new(title, PlotKind::Xy);
    chart.x_axis = Axis::new(x_label.into());
    chart.y_axis = Axis::new(y_label.into());
    if x_log {
        chart.x_axis.scale = ScaleKind::Log10;
    }
    if y_log {
        chart.y_axis.scale = ScaleKind::Log10;
    }

    let summarise_points = style != SeriesStyle::Columns;
    let points: Vec<DataPoint> = data
        .iter()
        .filter_map(|(x, reps)| {
            if reps.is_empty() {
                return None;
            }
            if summarise_points {
                let (mean, up, low) = summarise(reps, err)?;
                Some(DataPoint {
                    x: *x,
                    y: mean,
                    y_err: up,
                    y_err_low: low,
                    raw: reps.clone(),
                    ..Default::default()
                })
            } else {
                let mean = reps.iter().sum::<f64>() / reps.len() as f64;
                Some(DataPoint {
                    x: *x,
                    y: mean,
                    y_err: None,
                    y_err_low: None,
                    raw: reps.clone(),
                    ..Default::default()
                })
            }
        })
        .collect();
    if points.is_empty() {
        return chart;
    }

    let mut series = Series::new("Y", style, palette_color(0)).with_points(points);
    if style == SeriesStyle::Columns {
        // Individual replicates: no connecting line, no mean marker bar.
        series.marker = Marker::Circle;
        series.marker_size = 4;
    } else {
        series.marker = Marker::Circle;
    }
    chart.series.push(series);
    chart.legend = false;
    chart
}

/// One series per replicate index (each point is a single measurement).
pub fn xy_chart_by_replicate(
    title: impl Into<String>,
    x_label: impl Into<String>,
    y_label: impl Into<String>,
    data: &[(f64, Vec<f64>)],
    style: SeriesStyle,
    x_log: bool,
    y_log: bool,
) -> Chart {
    let mut chart = Chart::new(title, PlotKind::Xy);
    chart.x_axis = Axis::new(x_label.into());
    chart.y_axis = Axis::new(y_label.into());
    if x_log {
        chart.x_axis.scale = ScaleKind::Log10;
    }
    if y_log {
        chart.y_axis.scale = ScaleKind::Log10;
    }
    let n_reps = data.iter().map(|(_, r)| r.len()).max().unwrap_or(0);
    for rep in 0..n_reps {
        let pts: Vec<DataPoint> = data
            .iter()
            .filter_map(|(x, reps)| {
                let v = reps.get(rep).copied()?;
                Some(DataPoint {
                    x: *x,
                    y: v,
                    label: None,
                    raw: reps.clone(),
                    ..Default::default()
                })
            })
            .collect();
        if pts.is_empty() {
            continue;
        }
        let mut series = Series::new(
            format!("Replicate {}", rep + 1),
            style,
            palette_color(rep),
        )
        .with_points(pts);
        series.marker = default_marker(rep);
        chart.series.push(series);
    }
    chart
}

fn default_marker(i: usize) -> Marker {
    const MARKS: [Marker; 6] = [
        Marker::Circle,
        Marker::Square,
        Marker::Triangle,
        Marker::Diamond,
        Marker::InvertedTriangle,
        Marker::Cross,
    ];
    MARKS[i % MARKS.len()]
}

/// Jittered scatter / bar / box chart from labelled columns.
pub fn column_chart(
    title: impl Into<String>,
    y_label: impl Into<String>,
    columns: &[Vec<f64>],
    group_names: &[String],
    style: SeriesStyle,
    err: ErrorKind,
    y_log: bool,
) -> Chart {
    let kind = match style {
        SeriesStyle::Boxes => PlotKind::BoxAndWhisker,
        SeriesStyle::Bars | SeriesStyle::MeanBars => PlotKind::Bar,
        _ => PlotKind::Column,
    };
    let mut chart = Chart::new(title, kind);
    chart.x_axis = Axis::new("");
    chart.y_axis = Axis::new(y_label.into());
    if y_log {
        chart.y_axis.scale = ScaleKind::Log10;
    }
    chart.categories = group_names.to_vec();

    for (i, col) in columns.iter().enumerate() {
        if col.is_empty() {
            continue;
        }
        let name = group_names
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("Group {}", i + 1));
        let (mean, up, low) = summarise(col, err).unwrap_or((0.0, None, None));
        let point = DataPoint {
            x: i as f64,
            y: mean,
            y_err: up,
            y_err_low: low,
            label: Some(name.clone()),
            raw: col.clone(),
        };
        let mut s = Series::new(name, style, palette_color(0)).with_points(vec![point]);
        s.marker = default_marker(0);
        chart.series.push(s);
    }
    chart.legend = false;
    chart
}

/// Grouped chart: `data[row][group][replicate]`, one series per group.
pub fn grouped_chart(
    title: impl Into<String>,
    y_label: impl Into<String>,
    row_labels: &[String],
    group_names: &[String],
    data: &[Vec<Vec<f64>>],
    style: SeriesStyle,
    err: ErrorKind,
    y_log: bool,
) -> Chart {
    let kind = if matches!(style, SeriesStyle::Bars | SeriesStyle::MeanBars) {
        PlotKind::GroupedBar
    } else {
        PlotKind::Column
    };
    let mut chart = Chart::new(title, kind);
    chart.x_axis = Axis::new("");
    chart.y_axis = Axis::new(y_label.into());
    if y_log {
        chart.y_axis.scale = ScaleKind::Log10;
    }
    chart.categories = row_labels.to_vec();

    let n_groups = data.iter().map(|r| r.len()).max().unwrap_or(0);
    for g in 0..n_groups {
        let mut series = Series::new(
            group_names
                .get(g)
                .cloned()
                .unwrap_or_else(|| format!("Group {}", g + 1)),
            style,
            palette_color(g),
        );
        series.marker = default_marker(g);
        for (r, row) in data.iter().enumerate() {
            let Some(cell) = row.get(g) else { continue };
            if cell.is_empty() {
                continue;
            }
            let (mean, up, low) = summarise(cell, err).unwrap_or((0.0, None, None));
            series.points.push(DataPoint {
                x: r as f64,
                y: mean,
                y_err: up,
                y_err_low: low,
                label: row_labels.get(r).cloned(),
                raw: cell.clone(),
            });
        }
        chart.series.push(series);
    }
    chart
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xy_chart_summarises_replicates_with_error_bars() {
        let data = vec![
            (1.0, vec![1.0, 1.1, 1.2]),
            (2.0, vec![2.0, 2.2, 2.4]),
        ];
        let c = xy_chart(
            "t",
            "x",
            "y",
            &data,
            ErrorKind::Sem,
            SeriesStyle::LinesAndPoints,
            false,
            false,
        );
        // Replicates collapse into one summarised series.
        assert_eq!(c.series.len(), 1);
        assert_eq!(c.series[0].points.len(), 2);
        assert!(c.series[0].points[0].y_err.is_some());
        assert!((c.series[0].points[0].y - 1.1).abs() < 1e-9);
        // Range spans mean +/- SEM (raw extremes are not drawn).
        let (lo, hi) = c.y_data_range().unwrap();
        assert!(lo < 1.1 && hi > 2.2, "{lo} {hi}");
        assert!((c.series[0].points[0].y_err.unwrap() - 0.057735).abs() < 1e-4);
    }

    #[test]
    fn xy_chart_by_replicate_gives_one_series_each() {
        let data = vec![(1.0, vec![1.0, 2.0]), (2.0, vec![3.0, 4.0])];
        let c = xy_chart_by_replicate(
            "t",
            "x",
            "y",
            &data,
            SeriesStyle::LinesAndPoints,
            false,
            false,
        );
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.series[0].points.len(), 2);
        assert!((c.series[0].points[1].y - 3.0).abs() < 1e-9);
    }

    #[test]
    fn xy_chart_columns_style_keeps_raw_values() {
        let data = vec![(1.0, vec![1.0, 1.1, 1.2])];
        let c = xy_chart(
            "t",
            "x",
            "y",
            &data,
            ErrorKind::Sd,
            SeriesStyle::Columns,
            false,
            false,
        );
        assert_eq!(c.series.len(), 1);
        assert_eq!(c.series[0].points[0].raw.len(), 3);
        assert!(c.series[0].points[0].y_err.is_none());
    }

    #[test]
    fn box_options_are_categorical() {
        let cols = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let names = vec!["A".to_string(), "B".to_string()];
        let c = column_chart(
            "t",
            "y",
            &cols,
            &names,
            SeriesStyle::Boxes,
            ErrorKind::None,
            false,
        );
        assert_eq!(c.kind, PlotKind::BoxAndWhisker);
        assert_eq!(c.categories, names);
        let (lo, hi) = c.y_data_range().unwrap();
        assert!(lo <= 1.0 && hi >= 6.0);
    }

    #[test]
    fn grouped_chart_has_one_series_per_group() {
        let data = vec![
            vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            vec![vec![5.0, 6.0], vec![7.0, 8.0]],
        ];
        let rows = vec!["r1".to_string(), "r2".to_string()];
        let groups = vec!["g1".to_string(), "g2".to_string()];
        let c = grouped_chart(
            "t",
            "y",
            &rows,
            &groups,
            &data,
            SeriesStyle::Bars,
            ErrorKind::Sem,
            false,
        );
        assert_eq!(c.kind, PlotKind::GroupedBar);
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.series[0].points.len(), 2);
        assert!(c.needs_zero_baseline());
    }

    #[test]
    fn palette_cycles() {
        assert_eq!(palette_color(0), palette_color(12));
        assert_ne!(palette_color(0), palette_color(1));
    }
}
