//! Graph viewer: options, live rendering into an egui texture and PNG export.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{ComboBox, DragValue, RichText, TextureHandle, Ui};

use crate::analysis::{build_chart, Analysis, Graph};
use crate::data::{TableFlavor, TableInstance};
use crate::plot::{ErrorKind, PlotKind, SeriesStyle};

/// A request to export a graph, filled in by the panel and handled by the app.
#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub graph: usize,
    pub path: String,
    pub width: u32,
    pub height: u32,
}

/// Small LRU-ish cache of rendered chart textures.
#[derive(Default)]
pub struct TexCache {
    entries: Vec<(u64, TextureHandle)>,
    /// Canvas size used by the last render, for the axis readout.
    last_canvas: (u32, u32),
}

impl TexCache {
    /// Canvas size of the previous frame (falling back to a sane default).
    fn canvas(&self) -> (u32, u32) {
        if self.last_canvas.0 == 0 || self.last_canvas.1 == 0 {
            (1000, 700)
        } else {
            self.last_canvas
        }
    }

    pub fn get(&self, key: u64) -> Option<TextureHandle> {
        self.entries
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, t)| t.clone())
    }

    pub fn insert(&mut self, key: u64, tex: TextureHandle) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.push((key, tex));
        while self.entries.len() > 8 {
            let _ = self.entries.remove(0);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

fn hash_key(parts: &[u64]) -> u64 {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// Compact number formatting for axis readouts.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "n/a".into();
    }
    let a = v.abs();
    if a != 0.0 && (a < 1e-3 || a >= 1e5) {
        format!("{v:.3e}")
    } else {
        format!("{v:.4}")
    }
}

/// The chart kind implied by a series style.
pub fn kind_for(style: SeriesStyle, flavor: TableFlavor) -> PlotKind {
    match (style, flavor) {
        (SeriesStyle::Boxes, _) => PlotKind::BoxAndWhisker,
        (SeriesStyle::Bars | SeriesStyle::MeanBars, TableFlavor::Grouped) => PlotKind::GroupedBar,
        (SeriesStyle::Bars | SeriesStyle::MeanBars, TableFlavor::Contingency) => {
            PlotKind::GroupedBar
        }
        (SeriesStyle::Bars | SeriesStyle::MeanBars, _) => PlotKind::Bar,
        (SeriesStyle::Columns, TableFlavor::Xy) => PlotKind::Xy,
        (SeriesStyle::Columns, _) => PlotKind::Column,
        (_, TableFlavor::Xy) => PlotKind::Xy,
        (_, TableFlavor::Grouped) => PlotKind::Column,
        (_, _) => PlotKind::Column,
    }
}

fn style_label(s: SeriesStyle) -> &'static str {
    match s {
        SeriesStyle::Points => "Points only",
        SeriesStyle::Lines => "Lines only",
        SeriesStyle::LinesAndPoints => "Lines and points",
        SeriesStyle::Bars => "Bars",
        SeriesStyle::MeanBars => "Bars with mean marker",
        SeriesStyle::Boxes => "Box and whiskers",
        SeriesStyle::Columns => "Individual values (scatter)",
    }
}

/// The graph tab. Returns `true` when the graph set changed.
#[allow(clippy::too_many_arguments)]
pub fn graph_panel(
    ui: &mut Ui,
    ctx: &egui::Context,
    tables: &[TableInstance],
    selected_table: Option<usize>,
    analyses: &[Analysis],
    graphs: &mut Vec<Graph>,
    selected: &mut Option<usize>,
    cache: &mut TexCache,
    revision: u64,
    export_request: &mut Option<ExportRequest>,
) -> bool {
    let mut changed = false;
    let mut new_graph: Option<Graph> = None;
    let mut delete: Option<usize> = None;

    ui.horizontal_wrapped(|ui| {
        ui.heading("Graphs");
        ui.separator();
        let current = selected
            .and_then(|i| graphs.get(i))
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "none".into());
        ComboBox::from_id_source("graph_select")
            .selected_text(current)
            .width(220.0)
            .show_ui(ui, |ui| {
                for (i, g) in graphs.iter().enumerate() {
                    let label = if g.name.is_empty() {
                        format!("Graph {}", i + 1)
                    } else {
                        g.name.clone()
                    };
                    if ui
                        .selectable_label(*selected == Some(i), label)
                        .clicked()
                    {
                        *selected = Some(i);
                    }
                }
            });
        if ui
            .button("New")
            .on_hover_text("New graph of the current table")
            .clicked()
        {
            let base = selected_table
                .filter(|i| *i < tables.len())
                .or(if tables.is_empty() { None } else { Some(0) });
            if let Some(ti) = base {
                let t = &tables[ti];
                let flavor = t.flavor();
                let (kind, style, error) = match flavor {
                    TableFlavor::Xy => {
                        (PlotKind::Xy, SeriesStyle::LinesAndPoints, ErrorKind::Sem)
                    }
                    TableFlavor::Column | TableFlavor::Grouped => {
                        (PlotKind::Bar, SeriesStyle::Bars, ErrorKind::Sem)
                    }
                    TableFlavor::Contingency => {
                        (PlotKind::GroupedBar, SeriesStyle::Bars, ErrorKind::None)
                    }
                };
                new_graph = Some(Graph {
                    name: format!("Graph {}", graphs.len() + 1),
                    table: Some(ti),
                    analysis: None,
                    plot_kind: kind,
                    style,
                    error,
                    x_log: false,
                    y_log: false,
                    title: t.as_table().name().to_string(),
                    x_label: String::new(),
                    y_label: String::new(),
                    show_fit: true,
                    ..Default::default()
                });
            }
        }
        if let Some(i) = *selected {
            if ui.button("Duplicate").clicked() {
                if let Some(g) = graphs.get(i) {
                    let mut c = g.clone();
                    c.name = format!("{} copy", c.name);
                    new_graph = Some(c);
                }
            }
            if ui.button("Delete").clicked() {
                delete = Some(i);
            }
        }
    });

    if let Some(g) = new_graph {
        graphs.push(g);
        *selected = Some(graphs.len() - 1);
        *cache = TexCache::default();
        changed = true;
    }
    if let Some(i) = delete {
        if i < graphs.len() {
            graphs.remove(i);
            *selected = if graphs.is_empty() { None } else { Some(i.min(graphs.len() - 1)) };
            *cache = TexCache::default();
            changed = true;
        }
    }

    let Some(idx) = *selected else {
        ui.separator();
        ui.label(
            RichText::new(
                "No graph selected. Create one with 'New', or add an analysis and press its 'Graph' button.",
            )
            .weak(),
        );
        return changed;
    };
    if idx >= graphs.len() {
        return changed;
    }

    // ---- Options --------------------------------------------------------- //
    let flavor = graphs[idx]
        .table
        .and_then(|t| tables.get(t))
        .map(|t| t.flavor())
        .or_else(|| {
            graphs[idx]
                .analysis
                .and_then(|a| analyses.get(a))
                .and_then(|a| tables.get(a.table))
                .map(|t| t.flavor())
        })
        .unwrap_or(TableFlavor::Xy);

    ui.separator();
    let mut export_now = false;
    {
        let g = &mut graphs[idx];
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Title").strong());
            changed |= ui
                .add(egui::TextEdit::singleline(&mut g.title).desired_width(220.0))
                .changed();
            ui.label(RichText::new("X").strong());
            changed |= ui
                .add(egui::TextEdit::singleline(&mut g.x_label).desired_width(120.0))
                .changed();
            ui.label(RichText::new("Y").strong());
            changed |= ui
                .add(egui::TextEdit::singleline(&mut g.y_label).desired_width(120.0))
                .changed();
        });

        ui.horizontal_wrapped(|ui| {
            ComboBox::from_id_source("style_combo")
                .selected_text(style_label(g.style))
                .width(200.0)
                .show_ui(ui, |ui| {
                    for s in [
                        SeriesStyle::Points,
                        SeriesStyle::Lines,
                        SeriesStyle::LinesAndPoints,
                        SeriesStyle::Bars,
                        SeriesStyle::MeanBars,
                        SeriesStyle::Boxes,
                        SeriesStyle::Columns,
                    ] {
                        if ui
                            .selectable_value(&mut g.style, s, style_label(s))
                            .clicked()
                        {
                            g.plot_kind = kind_for(s, flavor);
                            changed = true;
                        }
                    }
                });

            if !matches!(flavor, TableFlavor::Xy) {
                ComboBox::from_id_source("error_combo")
                    .selected_text(format!("Error: {}", g.error.label()))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for e in ErrorKind::all() {
                            if ui
                                .selectable_value(&mut g.error, e, e.label())
                                .clicked()
                            {
                                changed = true;
                            }
                        }
                    });
            }

            changed |= ui.checkbox(&mut g.x_log, "Log X").changed();
            changed |= ui.checkbox(&mut g.y_log, "Log Y").changed();
            changed |= ui.checkbox(&mut g.legend, "Legend").changed();
            if flavor == TableFlavor::Xy {
                changed |= ui.checkbox(&mut g.show_fit, "Fit curve").changed();
                changed |= ui
                    .checkbox(&mut g.by_replicate, "Each replicate")
                    .on_hover_text("Draw one series per replicate instead of mean with error bars")
                    .changed();
            }
            if ui.button("Export PNG").clicked() {
                export_now = true;
            }
            let cached = cache.len();
            if ui
                .add_enabled(
                    cached > 0,
                    egui::Button::new(format!("Clear cache ({cached})")),
                )
                .on_hover_text("Drop the rendered graph images kept in GPU memory")
                .clicked()
            {
                cache.clear();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Canvas");
            changed |= ui
                .add(
                    DragValue::new(&mut g.width)
                        .speed(4.0)
                        .clamp_range(200..=4000)
                        .prefix("w "),
                )
                .changed();
            changed |= ui
                .add(
                    DragValue::new(&mut g.height)
                        .speed(4.0)
                        .clamp_range(150..=4000)
                        .prefix("h "),
                )
                .changed();
            ui.label(RichText::new("(used for PNG export)").weak());

            ui.separator();
            ui.label("Jitter");
            changed |= ui
                .add(
                    DragValue::new(&mut g.jitter)
                        .speed(0.01)
                        .clamp_range(0.0..=0.5)
                        .max_decimals(2),
                )
                .changed();

            ui.separator();
            ui.label("Data source:");
            let table_label = g
                .table
                .and_then(|t| tables.get(t))
                .map(|t| t.as_table().name().to_string())
                .unwrap_or_else(|| "(none)".into());
            ComboBox::from_id_source("graph_table_combo")
                .selected_text(table_label)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for (i, t) in tables.iter().enumerate() {
                        if ui
                            .selectable_label(
                                g.table == Some(i),
                                t.as_table().name().to_string(),
                            )
                            .clicked()
                        {
                            g.table = Some(i);
                            g.plot_kind = kind_for(g.style, t.flavor());
                            changed = true;
                        }
                    }
                });
            let analysis_label = g
                .analysis
                .and_then(|a| analyses.get(a))
                .map(|a| a.spec.label())
                .unwrap_or_else(|| "(none)".into());
            ComboBox::from_id_source("graph_analysis_combo")
                .selected_text(analysis_label)
                .width(180.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(g.analysis.is_none(), "(none)")
                        .clicked()
                    {
                        g.analysis = None;
                        changed = true;
                    }
                    for (i, a) in analyses.iter().enumerate() {
                        if ui
                            .selectable_label(g.analysis == Some(i), a.spec.label())
                            .clicked()
                        {
                            g.analysis = Some(i);
                            changed = true;
                        }
                    }
                });
        });
    }

    // ---- Render ---------------------------------------------------------- //
    ui.separator();

    // Clone is cheap (a few small fields) and avoids holding a borrow of
    // `graphs` while reading `tables`/`analyses`.
    let graph = graphs[idx].clone();
    let chart = build_chart(&graph, tables, analyses);

    // Axis readout, computed for the previous canvas size so that the layout
    // stays top-down (the image gets whatever space is left).
    if let Some(c) = &chart {
        let (cw, ch) = cache.canvas();
        let ((x0, x1), (y0, y1)) = crate::plot::render::axis_ranges(c, cw, ch);
        let (xt, yt) = crate::plot::render::axis_tick_labels(c, cw, ch);
        ui.collapsing("Axis details", |ui| {
            ui.label(
                RichText::new(format!(
                    "X: {} to {}   ({} ticks: {})",
                    num(x0),
                    num(x1),
                    xt.len(),
                    xt.join(", ")
                ))
                .small(),
            );
            ui.label(
                RichText::new(format!(
                    "Y: {} to {}   ({} ticks: {})",
                    num(y0),
                    num(y1),
                    yt.len(),
                    yt.join(", ")
                ))
                .small(),
            );
        });
    }

    let avail = ui.available_size();
    let w = ((avail.x - 6.0).clamp(240.0, 2200.0)) as u32;
    let h = ((avail.y - 10.0).clamp(180.0, 1600.0)) as u32;
    let key = hash_key(&[revision, idx as u64, w as u64, h as u64]);

    let texture = match cache.get(key) {
        Some(t) => Some(t),
        None => match chart.as_ref() {
            Some(c) => match crate::plot::render::render(c, w, h) {
                Ok(img) => {
                    let rgba = img.to_rgba();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [img.width as usize, img.height as usize],
                        &rgba,
                    );
                    let handle = ctx.load_texture(
                        format!("chart-{key}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    cache.insert(key, handle.clone());
                    cache.last_canvas = (w, h);
                    Some(handle)
                }
                Err(_) => None,
            },
            None => None,
        },
    };

    match texture {
        Some(t) => {
            ui.with_layout(
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.image(&t);
                },
            );
        }
        None => {
            ui.colored_label(
                crate::ui::error_color(ui),
                "Could not render this graph. Check that its table and analysis still exist.",
            );
        }
    }

    if export_now {
        let name = if graphs[idx].name.is_empty() {
            format!("graph{}", idx + 1)
        } else {
            graphs[idx]
                .name
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect()
        };
        *export_request = Some(ExportRequest {
            graph: idx,
            path: format!("{name}.png"),
            width: graphs[idx].width,
            height: graphs[idx].height,
        });
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_drives_plot_kind() {
        assert_eq!(
            kind_for(SeriesStyle::Bars, TableFlavor::Grouped),
            PlotKind::GroupedBar
        );
        assert_eq!(
            kind_for(SeriesStyle::Bars, TableFlavor::Column),
            PlotKind::Bar
        );
        assert_eq!(
            kind_for(SeriesStyle::Boxes, TableFlavor::Column),
            PlotKind::BoxAndWhisker
        );
        assert_eq!(
            kind_for(SeriesStyle::LinesAndPoints, TableFlavor::Xy),
            PlotKind::Xy
        );
        assert_eq!(
            kind_for(SeriesStyle::Columns, TableFlavor::Grouped),
            PlotKind::Column
        );
    }

    #[test]
    fn texture_cache_starts_empty() {
        let mut cache = TexCache::default();
        assert_eq!(cache.len(), 0);
        assert!(cache.get(42).is_none());
        cache.clear();
        assert_eq!(cache.len(), 0);
        // The axis readout falls back to a default canvas before the first render.
        assert_eq!(cache.canvas(), (1000, 700));
    }

    #[test]
    fn hash_key_is_stable_and_different() {
        assert_eq!(hash_key(&[1, 2, 3]), hash_key(&[1, 2, 3]));
        assert_ne!(hash_key(&[1, 2, 3]), hash_key(&[3, 2, 1]));
    }
}
