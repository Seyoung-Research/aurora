//! Raster renderer for [`Chart`]s, built directly on plotters' low-level
//! primitives.
//!
//! Drawing our own axes (instead of using `ChartBuilder`) keeps log scales,
//! categorical axes, box plots and grouped bars predictable, and lets the same
//! code serve both the GUI texture and PNG export.

use anyhow::Result;
use plotters::coord::Shift;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};

use super::scale::{self, ScaleKind};
use super::{Chart, DataPoint, Marker, PlotKind, Series, SeriesStyle};

const FONT: &str = "sans-serif";

const AXIS: RGBColor = RGBColor(70, 70, 70);
const FRAME: RGBColor = RGBColor(45, 45, 45);
const GRID: RGBColor = RGBColor(228, 228, 228);
const TEXT: RGBColor = RGBColor(24, 24, 24);
const MUTED: RGBColor = RGBColor(110, 110, 110);

const PAD_LEFT: i32 = 84;
const PAD_RIGHT: i32 = 30;
const PAD_TOP: i32 = 58;
const PAD_BOTTOM: i32 = 72;

/// A rendered raster image in RGB order (3 bytes per pixel).
#[derive(Clone, Debug)]
pub struct ChartImage {
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl ChartImage {
    /// Python/`image`-style flattened RGBA bytes (`ColorImage`-compatible).
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.rgb.len() / 3 * 4);
        for px in self.rgb.chunks_exact(3) {
            out.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
        out
    }

    /// `(r, g, b)` at a pixel, or `None` when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 3) as usize;
        Some((self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]))
    }
}

type Area<'a> = DrawingArea<BitMapBackend<'a>, Shift>;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Render a chart into an in-memory RGB buffer.
pub fn render(chart: &Chart, width: u32, height: u32) -> Result<ChartImage> {
    let width = width.max(160);
    let height = height.max(160);
    let mut buf = vec![0u8; width as usize * height as usize * 3];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let bg = rgb(chart.background);
        root.fill(&bg)?;
        draw_chart(&root, chart, width, height)?;
    }
    Ok(ChartImage {
        rgb: buf,
        width,
        height,
    })
}

/// Render a chart and encode it as PNG bytes.
pub fn export_png(chart: &Chart, width: u32, height: u32) -> Result<Vec<u8>> {
    let img = render(chart, width, height)?;
    super::png::encode_rgb(&img.rgb, img.width, img.height)
}

/// Render a chart and write it to `path` as PNG.
pub fn save_png(chart: &Chart, path: &str, width: u32, height: u32) -> Result<()> {
    let bytes = export_png(chart, width, height)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Data-space axis ranges that will be used for the given canvas size.
///
/// Useful for the UI (showing/editing the visible range) and for tests.
pub fn axis_ranges(chart: &Chart, width: u32, height: u32) -> ((f64, f64), (f64, f64)) {
    let l = layout(chart, width, height);
    (
        (
            scale::inverse(l.xr.0, l.xs),
            scale::inverse(l.xr.1, l.xs),
        ),
        (
            scale::inverse(l.yr.0, l.ys),
            scale::inverse(l.yr.1, l.ys),
        ),
    )
}

/// Tick label strings the renderer will place on each axis.
pub fn axis_tick_labels(chart: &Chart, width: u32, height: u32) -> (Vec<String>, Vec<String>) {
    let l = layout(chart, width, height);
    let xs: Vec<String> = scale::ticks(l.xr, l.xs, chart.x_axis.tick_count)
        .iter()
        .map(|t| {
            if is_categorical(chart.kind) {
                let idx = t.round() as i64;
                if idx >= 0 && (idx as usize) < chart.categories.len() {
                    return chart.categories[idx as usize].clone();
                }
            }
            scale::format_tick(scale::inverse(*t, l.xs))
        })
        .collect();
    let ys: Vec<String> = scale::ticks(l.yr, l.ys, chart.y_axis.tick_count)
        .iter()
        .map(|t| scale::format_tick(scale::inverse(*t, l.ys)))
        .collect();
    (xs, ys)
}

fn rgb(c: [u8; 3]) -> RGBColor {
    RGBColor(c[0], c[1], c[2])
}

/// Blend `c` over `bg` with `alpha` percent opacity.
fn blend(c: [u8; 3], bg: [u8; 3], alpha: u8) -> RGBColor {
    let a = (alpha.min(100) as f64) / 100.0;
    let mix = |x: u8, y: u8| ((x as f64 * a) + (y as f64 * (1.0 - a))).round() as u8;
    RGBColor(mix(c[0], bg[0]), mix(c[1], bg[1]), mix(c[2], bg[2]))
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Layout {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    /// Drawing-space range of the x axis.
    xr: (f64, f64),
    /// Drawing-space range of the y axis.
    yr: (f64, f64),
    xs: ScaleKind,
    ys: ScaleKind,
}

impl Layout {
    fn plot_width(&self) -> i32 {
        (self.x1 - self.x0).max(1)
    }

    fn plot_height(&self) -> i32 {
        (self.y1 - self.y0).max(1)
    }

    fn px_of(&self, v: f64) -> Option<i32> {
        let t = scale::transform(v, self.xs)?;
        Some(self.map(t, self.xr, self.x0, self.x1))
    }

    fn py_of(&self, v: f64) -> Option<i32> {
        let t = scale::transform(v, self.ys)?;
        Some(self.map(t, self.yr, self.y1, self.y0))
    }

    /// Pixel position of a value, clamped into the plot rectangle so error
    /// bars and lines are still visible when they run off the axis.
    fn px_clamped(&self, v: f64) -> Option<i32> {
        self.px_of(v).map(|p| p.clamp(self.x0, self.x1))
    }

    fn py_clamped(&self, v: f64) -> Option<i32> {
        self.py_of(v).map(|p| p.clamp(self.y0, self.y1))
    }

    fn map(&self, t: f64, r: (f64, f64), a: i32, b: i32) -> i32 {
        let span = r.1 - r.0;
        let f = if span.abs() < f64::EPSILON {
            0.5
        } else {
            (t - r.0) / span
        };
        a + (f * (b - a) as f64).round() as i32
    }

    /// Baseline (zero) in pixels, if zero is inside the range.
    fn baseline_y(&self) -> i32 {
        let zero = scale::transform(0.0, self.ys).unwrap_or(self.yr.0);
        let clamped = zero.clamp(self.yr.0.min(self.yr.1), self.yr.0.max(self.yr.1));
        self.map(clamped, self.yr, self.y1, self.y0)
    }

    /// Width of one category slot in pixels.
    fn category_slot(&self) -> f64 {
        let n = (self.xr.1 - self.xr.0).max(1.0);
        self.plot_width() as f64 / n
    }
}

fn is_categorical(kind: PlotKind) -> bool {
    !matches!(kind, PlotKind::Xy)
}

/// Compute an axis range in *drawing* (transformed) space.
///
/// - `specified`: an explicit data-space range from the user.
/// - `data`: the data-space extent that must be visible.
/// - `snap_nice`: expand the result to a "nice" grid (value axes only).
fn axis_range(
    specified: Option<(f64, f64)>,
    data: Option<(f64, f64)>,
    scale: ScaleKind,
    ticks_n: usize,
    snap_nice: bool,
    anchor_low: bool,
) -> (f64, f64) {
    if let Some((lo, hi)) = specified {
        if let (Some(a), Some(b)) = (scale::transform(lo, scale), scale::transform(hi, scale)) {
            if b > a {
                return (a, b);
            }
        }
    }

    let (lo, hi) = match data {
        Some((lo, hi)) if lo.is_finite() && hi.is_finite() => (lo, hi),
        _ => match scale {
            ScaleKind::Linear => (0.0, 1.0),
            ScaleKind::Log10 => (1.0, 10.0),
        },
    };

    match scale {
        ScaleKind::Linear => {
            let (mut lo, mut hi) = if (hi - lo).abs() < f64::EPSILON {
                (lo - 0.5, hi + 0.5)
            } else {
                let pad = (hi - lo) * 0.04;
                // Bars are anchored at the baseline: don't pad below it.
                let low = if anchor_low && lo >= 0.0 { lo } else { lo - pad };
                (low, hi + pad)
            };
            if snap_nice {
                let (a, b) = scale::nice_range(lo, hi, ScaleKind::Linear, ticks_n);
                lo = a;
                hi = b;
            }
            (lo, hi)
        }
        ScaleKind::Log10 => {
            // Snap to nice decades/mantissas in data space, which is what makes
            // log axes readable. Anything <= 0 is unusable on a log axis.
            let lo_pos = if lo > 0.0 { lo } else { 1e-3 };
            let hi_pos = if hi > lo_pos { hi } else { lo_pos * 10.0 };
            let (a, b) = if snap_nice {
                scale::nice_range(lo_pos, hi_pos, ScaleKind::Log10, ticks_n)
            } else {
                let pad = (hi_pos.log10() - lo_pos.log10()) * 0.03;
                (
                    10.0_f64.powf(lo_pos.log10() - pad),
                    10.0_f64.powf(hi_pos.log10() + pad),
                )
            };
            match (scale::transform(a, scale), scale::transform(b, scale)) {
                (Some(a), Some(b)) if b > a => (a, b),
                _ => (0.0, 1.0),
            }
        }
    }
}

fn layout(chart: &Chart, width: u32, height: u32) -> Layout {
    let x0 = PAD_LEFT.min(width as i32 / 3).max(10);
    let y0 = PAD_TOP.min(height as i32 / 3).max(10);
    let x1 = (width as i32 - PAD_RIGHT).max(x0 + 40);
    let y1 = (height as i32 - PAD_BOTTOM).max(y0 + 40);

    let xs = chart.x_axis.scale;
    let ys = chart.y_axis.scale;

    // --- x range -----------------------------------------------------------
    let xr = if is_categorical(chart.kind) {
        let n = chart
            .categories
            .len()
            .max(
                chart
                    .series
                    .iter()
                    .flat_map(|s| s.points.iter().map(|p| p.x))
                    .fold(0.0f64, f64::max)
                    .ceil() as usize
                    + 1,
            )
            .max(2);
        let data = match chart.x_axis.range {
            Some(r) => Some(r),
            None => Some((-0.7, n as f64 - 0.3)),
        };
        axis_range(
            chart.x_axis.range,
            data,
            xs,
            chart.x_axis.tick_count,
            false,
            false,
        )
    } else {
        axis_range(
            chart.x_axis.range,
            chart.x_data_range().or(Some((0.0, 1.0))),
            xs,
            chart.x_axis.tick_count,
            false,
            false,
        )
    };

    // --- y range -----------------------------------------------------------
    let mut y_data = chart.y_data_range().unwrap_or((0.0, 1.0));
    let anchor_zero = chart.needs_zero_baseline();
    if anchor_zero {
        y_data = (y_data.0.min(0.0), y_data.1.max(0.0));
    }
    let yr = axis_range(
        chart.y_axis.range,
        Some(y_data),
        ys,
        chart.y_axis.tick_count,
        true,
        anchor_zero,
    );

    Layout {
        x0,
        y0,
        x1,
        y1,
        xr,
        yr,
        xs,
        ys,
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers (all best-effort: a font failure never aborts a render)
// ---------------------------------------------------------------------------

/// Draw a line between two pixels.
fn line(da: &Area, a: (i32, i32), b: (i32, i32), c: RGBColor, w: u32) -> Result<()> {
    da.draw(&PathElement::new(vec![a, b], c.stroke_width(w.max(1))))?;
    Ok(())
}

/// Draw a closed polygon (the outline of a shape that is not filled).
fn poly(da: &Area, pts: &[(i32, i32)], c: RGBColor, w: u32) -> Result<()> {
    if pts.len() < 2 {
        return Ok(());
    }
    da.draw(&PathElement::new(
        pts.to_vec(),
        c.stroke_width(w.max(1)),
    ))?;
    Ok(())
}

/// Draw a rectangle, optionally filled and/or outlined.
fn rect(
    da: &Area,
    a: (i32, i32),
    b: (i32, i32),
    fill: Option<RGBColor>,
    stroke: Option<(RGBColor, u32)>,
) -> Result<()> {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    if let Some(f) = fill {
        da.draw(&Rectangle::new([(x0, y0), (x1, y1)], f.filled()))?;
    }
    if let Some((c, w)) = stroke {
        // Draw the outline as four lines so fill and stroke colours can differ.
        line(da, (x0, y0), (x1, y0), c, w)?;
        line(da, (x1, y0), (x1, y1), c, w)?;
        line(da, (x1, y1), (x0, y1), c, w)?;
        line(da, (x0, y1), (x0, y0), c, w)?;
    }
    Ok(())
}

/// Draw a marker shape (circle, box, triangle, ... or a line-based shape) at a
/// pixel position.
fn marker(da: &Area, at: (i32, i32), m: Marker, size: i32, c: RGBColor) -> Result<()> {
    let s = size.max(2);
    match m {
        Marker::None => {}
        Marker::Circle => {
            da.draw(&Circle::new(at, s, c.filled()))?;
        }
        Marker::Square => {
            da.draw(&Rectangle::new(
                [(at.0 - s, at.1 - s), (at.0 + s, at.1 + s)],
                c.filled(),
            ))?;
        }
        Marker::Triangle => {
            da.draw(&Polygon::new(
                vec![(at.0, at.1 - s), (at.0 - s, at.1 + s), (at.0 + s, at.1 + s)],
                c.filled(),
            ))?;
        }
        Marker::InvertedTriangle => {
            da.draw(&Polygon::new(
                vec![(at.0, at.1 + s), (at.0 - s, at.1 - s), (at.0 + s, at.1 - s)],
                c.filled(),
            ))?;
        }
        Marker::Diamond => {
            da.draw(&Polygon::new(
                vec![
                    (at.0, at.1 - s),
                    (at.0 + s, at.1),
                    (at.0, at.1 + s),
                    (at.0 - s, at.1),
                ],
                c.filled(),
            ))?;
        }
        Marker::Cross => {
            line(da, (at.0 - s, at.1 - s), (at.0 + s, at.1 + s), c, 2)?;
            line(da, (at.0 - s, at.1 + s), (at.0 + s, at.1 - s), c, 2)?;
        }
        Marker::Plus => {
            line(da, (at.0 - s, at.1), (at.0 + s, at.1), c, 2)?;
            line(da, (at.0, at.1 - s), (at.0, at.1 + s), c, 2)?;
        }
    }
    Ok(())
}

/// Draw text; failures (e.g. no usable system font) are logged, never fatal.
fn text(
    da: &Area,
    s: &str,
    at: (i32, i32),
    size: i32,
    color: RGBColor,
    anchor: (HPos, VPos),
) {
    let mut style = (FONT, size).into_font().color(&color);
    style.pos = Pos::new(anchor.0, anchor.1);
    if let Err(e) = da.draw_text(s, &style, at) {
        log::debug!("plot text failed: {e}");
    }
}

const ANCHOR_CENTER: (HPos, VPos) = (HPos::Center, VPos::Center);
const ANCHOR_TOP: (HPos, VPos) = (HPos::Center, VPos::Top);
const ANCHOR_LEFT_MID: (HPos, VPos) = (HPos::Left, VPos::Center);
const ANCHOR_RIGHT_MID: (HPos, VPos) = (HPos::Right, VPos::Center);
const ANCHOR_RIGHT_BOTTOM: (HPos, VPos) = (HPos::Right, VPos::Bottom);
const ANCHOR_LEFT_TOP: (HPos, VPos) = (HPos::Left, VPos::Top);

// ---------------------------------------------------------------------------
// The chart
// ---------------------------------------------------------------------------

fn draw_chart(da: &Area, chart: &Chart, width: u32, height: u32) -> Result<()> {
    let l = layout(chart, width, height);
    let bg = chart.background;

    // Plot area ------------------------------------------------------------
    if chart.grid {
        for t in scale::ticks(l.yr, l.ys, chart.y_axis.tick_count) {
            if let Some(y) = l.py_of(t) {
                if y > l.y0 && y < l.y1 {
                    line(da, (l.x0, y), (l.x1, y), GRID, 1)?;
                }
            }
        }
    }

    // Title ----------------------------------------------------------------
    if !chart.title.is_empty() {
        text(
            da,
            &chart.title,
            ((l.x0 + l.x1) / 2, 16),
            18,
            TEXT,
            ANCHOR_TOP,
        );
    }

    // Axes -----------------------------------------------------------------
    let y_ticks = scale::ticks(l.yr, l.ys, chart.y_axis.tick_count);
    for t in &y_ticks {
        let y = l.map(*t, l.yr, l.y1, l.y0);
        line(da, (l.x0 - 5, y), (l.x0, y), AXIS, 1)?;
        text(
            da,
            &scale::format_tick(scale::inverse(*t, l.ys)),
            (l.x0 - 9, y),
            13,
            MUTED,
            ANCHOR_RIGHT_MID,
        );
    }

    let x_ticks = scale::ticks(l.xr, l.xs, chart.x_axis.tick_count);
    for t in &x_ticks {
        let x = l.map(*t, l.xr, l.x0, l.x1);
        line(da, (x, l.y1), (x, l.y1 + 5), AXIS, 1)?;
        let label = if is_categorical(chart.kind) {
            let idx = t.round() as i64;
            if idx >= 0 && (idx as usize) < chart.categories.len() {
                chart.categories[idx as usize].clone()
            } else {
                scale::format_tick(scale::inverse(*t, l.xs))
            }
        } else {
            scale::format_tick(scale::inverse(*t, l.xs))
        };
        text(da, &label, (x, l.y1 + 8), 13, MUTED, ANCHOR_TOP);
    }

    if chart.frame {
        rect(
            da,
            (l.x0, l.y0),
            (l.x1, l.y1),
            None,
            Some((FRAME, 1)),
        )?;
    } else {
        line(da, (l.x0, l.y1), (l.x1, l.y1), AXIS, 1)?;
        line(da, (l.x0, l.y0), (l.x0, l.y1), AXIS, 1)?;
    }

    // Axis titles ----------------------------------------------------------
    if !chart.x_axis.label.is_empty() {
        text(
            da,
            &chart.x_axis.label,
            ((l.x0 + l.x1) / 2, l.y1 + 34),
            15,
            TEXT,
            ANCHOR_TOP,
        );
    }
    if !chart.y_axis.label.is_empty() {
        // Horizontal title above the y axis (reads well at any canvas size).
        text(
            da,
            &chart.y_axis.label,
            (l.x0, l.y0 - 24),
            15,
            TEXT,
            ANCHOR_LEFT_TOP,
        );
    }

    // Series ---------------------------------------------------------------
    draw_series(da, chart, &l, bg)?;

    // Legend ---------------------------------------------------------------
    if chart.legend {
        draw_legend(da, chart, &l)?;
    }

    Ok(())
}

fn draw_series(da: &Area, chart: &Chart, l: &Layout, bg: [u8; 3]) -> Result<()> {
    // Bars need to be drawn before points/lines so markers sit on top.
    let bar_count = chart
        .series
        .iter()
        .filter(|s| matches!(s.style, SeriesStyle::Bars | SeriesStyle::MeanBars))
        .count();

    if bar_count > 0 {
        let mut bar_idx = 0;
        for s in &chart.series {
            if matches!(s.style, SeriesStyle::Bars | SeriesStyle::MeanBars) {
                draw_bars(da, s, l, bar_idx, bar_count, bg)?;
                bar_idx += 1;
            }
        }
    }

    for s in &chart.series {
        match s.style {
            SeriesStyle::Columns => draw_columns(da, s, l, bg)?,
            SeriesStyle::Boxes => draw_boxes(da, s, l, bg)?,
            SeriesStyle::Bars | SeriesStyle::MeanBars => draw_errors(da, s, l)?,
            SeriesStyle::Points => {
                draw_errors(da, s, l)?;
                draw_markers(da, s, l)?;
            }
            SeriesStyle::Lines => {
                draw_errors(da, s, l)?;
                draw_line_series(da, s, l)?;
            }
            SeriesStyle::LinesAndPoints => {
                draw_errors(da, s, l)?;
                draw_line_series(da, s, l)?;
                draw_markers(da, s, l)?;
            }
        }
    }
    Ok(())
}

fn points_sorted(s: &Series) -> Vec<&DataPoint> {
    let mut v: Vec<&DataPoint> = s.points.iter().collect();
    v.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    v
}

fn draw_line_series(da: &Area, s: &Series, l: &Layout) -> Result<()> {
    let pts: Vec<(i32, i32)> = points_sorted(s)
        .iter()
        .filter_map(|p| {
            let x = l.px_clamped(p.x)?;
            let y = l.py_of(p.y)?;
            Some((x, y))
        })
        .collect();
    poly(da, &pts, rgb(s.color), s.line_width)?;
    Ok(())
}

fn draw_markers(da: &Area, s: &Series, l: &Layout) -> Result<()> {
    let c = rgb(s.color);
    for p in &s.points {
        if let (Some(x), Some(y)) = (l.px_clamped(p.x), l.py_of(p.y)) {
            marker(da, (x, y), s.marker, s.marker_size as i32, c)?;
        }
    }
    Ok(())
}

fn draw_errors(da: &Area, s: &Series, l: &Layout) -> Result<()> {
    let c = rgb(s.color);
    for p in &s.points {
        let Some((lo, hi)) = p.err_extents() else {
            continue;
        };
        let Some(x) = l.px_clamped(p.x) else { continue };
        let (Some(y_lo), Some(y_hi)) = (l.py_of(lo), l.py_of(hi)) else {
            continue;
        };
        line(da, (x, y_lo), (x, y_hi), c, 2)?;
        let cap = 5;
        line(da, (x - cap, y_lo), (x + cap, y_lo), c, 2)?;
        line(da, (x - cap, y_hi), (x + cap, y_hi), c, 2)?;
    }
    Ok(())
}

fn draw_bars(
    da: &Area,
    s: &Series,
    l: &Layout,
    index: usize,
    total: usize,
    bg: [u8; 3],
) -> Result<()> {
    let fill = blend(s.color, bg, s.fill_alpha);
    let stroke = rgb(s.color);
    let baseline = l.baseline_y();
    // 0.8 of a category slot is shared between the series in the group; the
    // value below is the half-width, so bars never overlap.
    let slot = l.category_slot() * 0.8 / total as f64;
    let bar_half = ((slot * 0.42).max(1.0)) as i32;

    for p in &s.points {
        let Some(cx) = l.px_of(p.x) else { continue };
        let Some(top) = l.py_clamped(p.y) else { continue };
        let offset = -slot * (total as f64) / 2.0 + slot * (index as f64 + 0.5);
        let center = cx + offset.round() as i32;
        rect(
            da,
            (center - bar_half, top),
            (center + bar_half, baseline),
            Some(fill),
            Some((stroke, 1)),
        )?;
        if s.style == SeriesStyle::MeanBars {
            marker(da, (center, top), Marker::Circle, 2, stroke)?;
        }
    }
    Ok(())
}

fn draw_columns(da: &Area, s: &Series, l: &Layout, bg: [u8; 3]) -> Result<()> {
    let c = rgb(s.color);
    let mean_color = blend(s.color, bg, 55);
    let jitter = 0.28f64;

    for p in &s.points {
        let Some(cx) = l.px_of(p.x) else { continue };
        // Jittered raw values.
        let k = p.raw.len();
        for (i, v) in p.raw.iter().enumerate() {
            let Some(y) = l.py_clamped(*v) else { continue };
            let frac = if k <= 1 {
                0.5
            } else {
                i as f64 / (k - 1) as f64
            };
            let dx = ((frac - 0.5) * 2.0 * jitter * l.category_slot()) as i32;
            marker(da, (cx + dx, y), s.marker, (s.marker_size as i32).min(4), c)?;
        }
        // Mean +/- error as a wide bar.
        if let Some((lo, hi)) = p.err_extents() {
            let half = (l.category_slot() * 0.22) as i32;
            if let (Some(y_lo), Some(y_hi)) = (l.py_of(lo), l.py_of(hi)) {
                let w = 10.max(half);
                line(da, (cx, y_lo), (cx, y_hi), c, 1)?;
                line(da, (cx - w, y_lo), (cx + w, y_lo), c, 1)?;
                line(da, (cx - w, y_hi), (cx + w, y_hi), c, 1)?;
            }
        }
        if let Some(y) = l.py_of(p.y) {
            let half = (l.category_slot() * 0.18).max(6.0) as i32;
            line(da, (cx - half, y), (cx + half, y), mean_color, 3)?;
        }
    }
    Ok(())
}

/// Tukey-style box statistics.
struct BoxStats {
    q1: f64,
    median: f64,
    q3: f64,
    whisker_lo: f64,
    whisker_hi: f64,
    outliers: Vec<f64>,
}

fn box_stats(values: &[f64]) -> Option<BoxStats> {
    if values.is_empty() {
        return None;
    }
    let s = crate::stats::descriptive::calculate_descriptive(values)?;
    let iqr = s.q3 - s.q1;
    let lo_fence = s.q1 - 1.5 * iqr;
    let hi_fence = s.q3 + 1.5 * iqr;
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let inside: Vec<f64> = sorted
        .iter()
        .copied()
        .filter(|v| *v >= lo_fence && *v <= hi_fence)
        .collect();
    let whisker_lo = inside.first().copied().unwrap_or(s.q1);
    let whisker_hi = inside.last().copied().unwrap_or(s.q3);
    let outliers: Vec<f64> = sorted
        .iter()
        .copied()
        .filter(|v| *v < lo_fence || *v > hi_fence)
        .collect();
    Some(BoxStats {
        q1: s.q1,
        median: s.median,
        q3: s.q3,
        whisker_lo,
        whisker_hi,
        outliers,
    })
}

fn draw_boxes(da: &Area, s: &Series, l: &Layout, bg: [u8; 3]) -> Result<()> {
    let stroke = rgb(s.color);
    let fill = blend(s.color, bg, 30);

    for p in &s.points {
        let Some(cx) = l.px_of(p.x) else { continue };
        let stats = if p.raw.is_empty() {
            // Fall back to a degenerate box at the summary point.
            p.err_extents().map(|(lo, hi)| BoxStats {
                q1: lo,
                median: p.y,
                q3: hi,
                whisker_lo: lo,
                whisker_hi: hi,
                outliers: Vec::new(),
            })
        } else {
            box_stats(&p.raw)
        };
        let Some(st) = stats else { continue };

        let half = (l.category_slot() * 0.4).max(8.0) as i32;
        let (Some(q1), Some(q3), Some(med), Some(wlo), Some(whi)) = (
            l.py_clamped(st.q1),
            l.py_clamped(st.q3),
            l.py_clamped(st.median),
            l.py_clamped(st.whisker_lo),
            l.py_clamped(st.whisker_hi),
        ) else {
            continue;
        };

        rect(
            da,
            (cx - half, q3),
            (cx + half, q1),
            Some(fill),
            Some((stroke, 1)),
        )?;
        line(da, (cx - half, med), (cx + half, med), stroke, 2)?;
        // whiskers
        let cap = (half / 2).max(4);
        line(da, (cx, q1), (cx, wlo), stroke, 1)?;
        line(da, (cx, q3), (cx, whi), stroke, 1)?;
        line(da, (cx - cap, wlo), (cx + cap, wlo), stroke, 1)?;
        line(da, (cx - cap, whi), (cx + cap, whi), stroke, 1)?;
        for o in &st.outliers {
            if let Some(y) = l.py_clamped(*o) {
                marker(da, (cx, y), Marker::Circle, 2, stroke)?;
            }
        }
    }
    Ok(())
}

fn draw_legend(da: &Area, chart: &Chart, l: &Layout) -> Result<()> {
    // Only meaningful with a name to show.
    let entries: Vec<&Series> = chart
        .series
        .iter()
        .filter(|s| !s.name.is_empty() && s.points.len() > 0)
        .collect();
    if entries.is_empty() {
        return Ok(());
    }

    let row_h = 20;
    let char_w = 7;
    let max_label = entries
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(0) as i32;
    let box_w = 34 + max_label * char_w + 14;
    let box_h = entries.len() as i32 * row_h + 12;
    let bx1 = l.x1 - 12;
    let by0 = l.y0 + 12;
    let bx0 = bx1 - box_w;
    let by1 = by0 + box_h;

    let white = RGBColor(255, 255, 255);
    let border = RGBColor(180, 180, 180);
    rect(da, (bx0, by0), (bx1, by1), Some(white), Some((border, 1)))?;

    for (i, s) in entries.iter().enumerate() {
        let cy = by0 + 6 + row_h * i as i32 + row_h / 2;
        let cx = bx0 + 22;
        let c = rgb(s.color);
        match s.marker {
            Marker::None => line(da, (cx - 10, cy), (cx + 10, cy), c, 2)?,
            m => marker(da, (cx, cy), m, 4, c)?,
        }
        text(
            da,
            &s.name,
            (bx0 + 40, cy),
            13,
            TEXT,
            ANCHOR_LEFT_MID,
        );
    }
    let _ = ANCHOR_RIGHT_BOTTOM;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::{column_chart, grouped_chart, xy_chart, xy_chart_by_replicate, ErrorKind};

    fn non_blank(img: &ChartImage) -> usize {
        let bg = (255u8, 255u8, 255u8);
        let mut n = 0;
        for y in 0..img.height {
            for x in 0..img.width {
                if img.pixel(x, y).map(|p| p != bg).unwrap_or(false) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn renders_xy_chart() {
        let data = vec![
            (1.0, vec![1.0, 1.2, 0.9]),
            (2.0, vec![2.1, 1.9, 2.2]),
            (3.0, vec![3.2, 2.8, 3.0]),
        ];
        let c = xy_chart(
            "Growth",
            "Time (h)",
            "OD600",
            &data,
            ErrorKind::Sem,
            SeriesStyle::LinesAndPoints,
            false,
            false,
        );
        let img = render(&c, 640, 480).unwrap();
        assert_eq!(img.width, 640);
        assert_eq!(img.height, 480);
        assert_eq!(img.rgb.len(), 640 * 480 * 3);
        // Something was actually drawn.
        assert!(non_blank(&img) > 500, "blank image");
    }

    #[test]
    fn renders_bar_chart_with_baseline() {
        let cols = vec![vec![1.0, 1.2, 0.8], vec![3.0, 3.4, 2.8]];
        let names = vec!["A".to_string(), "B".to_string()];
        let c = column_chart(
            "Means",
            "Value",
            &cols,
            &names,
            SeriesStyle::Bars,
            ErrorKind::Sem,
            false,
        );
        let img = render(&c, 600, 400).unwrap();
        assert!(non_blank(&img) > 500);
    }

    #[test]
    fn renders_box_and_column_plots() {
        let cols = vec![
            vec![1.0, 2.0, 3.0, 4.0, 100.0],
            vec![2.0, 3.0, 4.0],
        ];
        let names = vec!["Control".to_string(), "Treated".to_string()];
        for style in [SeriesStyle::Boxes, SeriesStyle::Columns] {
            let c = column_chart("t", "y", &cols, &names, style, ErrorKind::Sem, false);
            let img = render(&c, 600, 400).unwrap();
            assert!(non_blank(&img) > 300, "{style:?} blank");
        }
    }

    #[test]
    fn renders_grouped_bars_and_log_axis() {
        let data = vec![
            vec![vec![1.0, 1.1], vec![10.0, 11.0]],
            vec![vec![2.0, 2.1], vec![20.0, 21.0]],
        ];
        let rows = vec!["Day 1".to_string(), "Day 2".to_string()];
        let groups = vec!["A".to_string(), "B".to_string()];
        let c = grouped_chart(
            "Grouped",
            "Concentration",
            &rows,
            &groups,
            &data,
            SeriesStyle::Bars,
            ErrorKind::Sem,
            true,
        );
        let img = render(&c, 700, 500).unwrap();
        assert!(non_blank(&img) > 500);
    }

    #[test]
    fn handles_empty_chart_without_panicking() {
        let c = Chart::new("Empty", PlotKind::Xy);
        let img = render(&c, 400, 300).unwrap();
        assert_eq!(img.rgb.len(), 400 * 300 * 3);
    }

    #[test]
    fn log_axis_skips_nonpositive_points() {
        let mut c = Chart::new("Log", PlotKind::Xy);
        c.y_axis.scale = ScaleKind::Log10;
        let s = Series::new("s", SeriesStyle::LinesAndPoints, [0, 0, 0]).with_points(vec![
            DataPoint::new(1.0, 1.0),
            DataPoint::new(2.0, 0.0), // off-scale
            DataPoint::new(3.0, 100.0),
        ]);
        c.series.push(s);
        let img = render(&c, 400, 300).unwrap();
        assert!(non_blank(&img) > 100);
    }

    #[test]
    fn png_export_produces_image_bytes() {
        let data = vec![(1.0, vec![1.0]), (2.0, vec![2.0])];
        let c = xy_chart("t", "x", "y", &data, ErrorKind::None, SeriesStyle::Lines, false, false);
        let png = export_png(&c, 300, 200).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn linear_axis_snaps_to_nice_range() {
        let data = vec![
            (0.0, vec![0.05, 0.08]),
            (2.0, vec![0.3, 0.35]),
            (4.0, vec![0.9, 1.0]),
        ];
        let c = xy_chart_by_replicate(
            "t",
            "Time",
            "OD",
            &data,
            SeriesStyle::LinesAndPoints,
            false,
            false,
        );
        let (xr, yr) = axis_ranges(&c, 720, 520);
        // X hugs the data (0..4) with a 4% pad so edge markers stay visible.
        assert!((xr.0 + 0.16).abs() < 1e-9, "xr {xr:?}");
        assert!((xr.1 - 4.16).abs() < 1e-9, "xr {xr:?}");
        assert!((yr.0 - 0.0).abs() < 1e-9, "yr {yr:?}");
        assert!((yr.1 - 1.2).abs() < 1e-9, "yr {yr:?}");

        let (_, ylabels) = axis_tick_labels(&c, 720, 520);
        assert!(ylabels.contains(&"1".to_string()), "{ylabels:?}");
        assert!(ylabels.contains(&"0.5".to_string()), "{ylabels:?}");
    }

    #[test]
    fn bar_chart_starts_at_zero() {
        let cols = vec![vec![1.0, 1.2], vec![3.0, 3.4]];
        let names = vec!["A".to_string(), "B".to_string()];
        let c = column_chart(
            "t",
            "y",
            &cols,
            &names,
            SeriesStyle::Bars,
            ErrorKind::Sem,
            false,
        );
        let (_, yr) = axis_ranges(&c, 700, 500);
        assert!(yr.0 >= 0.0, "bars should not dip below zero: {yr:?}");
        assert!(yr.1 > 3.4);
    }

    #[test]
    fn log_y_axis_uses_decades_and_keeps_data_positive() {
        let data = vec![
            (1.0, vec![0.5]),
            (2.0, vec![5.0]),
            (3.0, vec![500.0]),
        ];
        let c = xy_chart(
            "t",
            "x",
            "y",
            &data,
            ErrorKind::None,
            SeriesStyle::LinesAndPoints,
            false,
            true,
        );
        let (_, yr) = axis_ranges(&c, 700, 500);
        assert!(yr.0 > 0.0, "{yr:?}");
        assert!(yr.0 <= 0.5 && yr.1 >= 500.0, "{yr:?}");
        let (_, ylabels) = axis_tick_labels(&c, 700, 500);
        // Several decades -> whole decades are labelled.
        assert!(ylabels.contains(&"10".to_string()), "{ylabels:?}");
        assert!(ylabels.contains(&"100".to_string()), "{ylabels:?}");
    }

    #[test]
    fn to_rgba_expands_pixels() {
        let img = ChartImage {
            rgb: vec![1, 2, 3, 4, 5, 6],
            width: 2,
            height: 1,
        };
        assert_eq!(img.to_rgba(), vec![1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(img.pixel(1, 0), Some((4, 5, 6)));
        assert_eq!(img.pixel(2, 0), None);
    }

    /// Writes sample images for visual inspection.
    ///
    /// Run with `GP_WRITE_SAMPLES=1 cargo test plot::render::tests::write_sample_pngs`.
    #[test]
    fn write_sample_pngs() {
        let Ok(dir) = std::env::var("GP_WRITE_SAMPLES") else {
            return;
        };
        let dir = if dir.is_empty() {
            "samples".to_string()
        } else {
            dir
        };
        std::fs::create_dir_all(&dir).unwrap();

        let xy = vec![
            (0.1, vec![1.2, 1.35, 1.1]),
            (1.0, vec![5.5, 5.8, 5.2]),
            (3.0, vec![9.1, 8.8, 9.4]),
            (10.0, vec![12.1, 11.9, 12.5]),
            (30.0, vec![12.6, 12.9, 12.2]),
        ];
        let dose = xy_chart(
            "Dose response",
            "Concentration (nM)",
            "Response (AU)",
            &xy,
            ErrorKind::Sem,
            SeriesStyle::LinesAndPoints,
            true,
            false,
        );
        save_png(&dose, &format!("{dir}/xy_log_x.png"), 720, 520).unwrap();

        let reps = xy_chart_by_replicate(
            "Replicates",
            "Time (h)",
            "OD600",
            &[(0.0, vec![0.05, 0.08]), (2.0, vec![0.3, 0.35]), (4.0, vec![0.9, 1.0])],
            SeriesStyle::LinesAndPoints,
            false,
            false,
        );
        save_png(&reps, &format!("{dir}/xy_replicates.png"), 720, 520).unwrap();

        let cols = vec![
            vec![1.0, 1.4, 1.2, 1.6, 1.1],
            vec![2.4, 2.9, 2.1, 3.4, 2.7],
            vec![4.2, 5.1, 4.6, 3.9, 4.8],
        ];
        let names = vec![
            "Control".to_string(),
            "Low dose".to_string(),
            "High dose".to_string(),
        ];
        let box_plot = column_chart(
            "Box plot",
            "Response",
            &cols,
            &names,
            SeriesStyle::Boxes,
            ErrorKind::None,
            false,
        );
        save_png(&box_plot, &format!("{dir}/boxes.png"), 720, 520).unwrap();

        let bars = column_chart(
            "Bar with SEM",
            "Response",
            &cols,
            &names,
            SeriesStyle::Bars,
            ErrorKind::Sem,
            false,
        );
        save_png(&bars, &format!("{dir}/bars.png"), 720, 520).unwrap();

        let scatter = column_chart(
            "Column scatter",
            "Response",
            &cols,
            &names,
            SeriesStyle::Columns,
            ErrorKind::Sem,
            false,
        );
        save_png(&scatter, &format!("{dir}/scatter.png"), 720, 520).unwrap();

        let grouped = vec![
            vec![vec![1.0, 1.2], vec![3.0, 3.4], vec![0.4, 0.55]],
            vec![vec![2.1, 2.3], vec![4.4, 4.9], vec![0.9, 1.05]],
            vec![vec![3.4, 3.6], vec![6.2, 6.6], vec![1.6, 1.8]],
        ];
        let rows = vec!["Day 1".to_string(), "Day 2".to_string(), "Day 3".to_string()];
        let groups = vec!["PBS".to_string(), "Drug A".to_string(), "Drug B".to_string()];
        let grouped_chart = grouped_chart(
            "Grouped bars",
            "Concentration (uM)",
            &rows,
            &groups,
            &grouped,
            SeriesStyle::Bars,
            ErrorKind::Sem,
            false,
        );
        save_png(&grouped_chart, &format!("{dir}/grouped_bars.png"), 760, 520).unwrap();

        let log_y = column_chart(
            "Log Y axis",
            "Titre (log10)",
            &cols,
            &names,
            SeriesStyle::Columns,
            ErrorKind::Sd,
            true,
        );
        save_png(&log_y, &format!("{dir}/log_y.png"), 720, 520).unwrap();
    }
}
