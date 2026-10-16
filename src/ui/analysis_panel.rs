//! Analysis list, per-analysis settings and results rendering.

use egui::{ComboBox, DragValue, Grid, RichText, ScrollArea, Slider, Ui};

use crate::analysis::{Analysis, AnalysisSpec, Graph};
use crate::data::{TableFlavor, TableInstance};
use crate::plot::{ErrorKind, PlotKind, SeriesStyle};
use crate::stats::comparison::TTestKind;
use crate::ui::{error_color, success_color};

/// Deferred edits, applied after the immutable UI pass.
enum Action {
    Add(AnalysisSpec),
    Run(usize),
    NewGraph(usize),
    Delete(usize),
}

/// Display names of the groups (columns) of a table.
pub fn group_names(table: &TableInstance) -> Vec<String> {
    match table {
        TableInstance::Column(t) => t
            .columns
            .iter()
            .enumerate()
            .map(|(i, _)| {
                t.group_names
                    .get(i)
                    .cloned()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("Group {}", i + 1))
            })
            .collect(),
        TableInstance::Grouped(t) => t
            .group_names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if n.is_empty() {
                    format!("Group {}", i + 1)
                } else {
                    n.clone()
                }
            })
            .collect(),
        TableInstance::Contingency(t) => t.col_labels.clone(),
        TableInstance::Xy(t) => {
            let n = t.data.iter().map(|(_, r)| r.len()).max().unwrap_or(1);
            (0..n).map(|i| format!("Y{}", i + 1)).collect()
        }
    }
}

fn default_graph_for(spec_label: &str, flavor: TableFlavor, analysis: usize) -> Graph {
    let (kind, style, error) = match flavor {
        TableFlavor::Xy => (PlotKind::Xy, SeriesStyle::LinesAndPoints, ErrorKind::Sem),
        TableFlavor::Column => (PlotKind::Bar, SeriesStyle::Bars, ErrorKind::Sem),
        TableFlavor::Grouped => (PlotKind::GroupedBar, SeriesStyle::Bars, ErrorKind::Sem),
        TableFlavor::Contingency => (PlotKind::GroupedBar, SeriesStyle::Bars, ErrorKind::None),
    };
    Graph {
        name: spec_label.to_string(),
        table: None,
        analysis: Some(analysis),
        plot_kind: kind,
        style,
        error,
        x_log: false,
        y_log: false,
        title: spec_label.to_string(),
        x_label: String::new(),
        y_label: String::new(),
        show_fit: true,
        ..Default::default()
    }
}

/// Analysis panel for one table. Returns `true` when something changed.
pub fn analysis_panel(
    ui: &mut Ui,
    tables: &[TableInstance],
    analyses: &mut Vec<Analysis>,
    graphs: &mut Vec<Graph>,
    table_index: usize,
    selected: &mut Option<usize>,
    revision: &mut u64,
) -> bool {
    let mut changed = false;
    let mut actions: Vec<Action> = Vec::new();

    let Some(table) = tables.get(table_index) else {
        ui.label("Select a table first.");
        return false;
    };
    let names = group_names(table);
    let flavor = table.flavor();
    let table_name = table.as_table().name().to_string();
    let menu = AnalysisSpec::menu_for(flavor, crate::analysis::table_group_count(table));

    ui.horizontal_wrapped(|ui| {
        ui.heading("Analyses");
        ui.label(RichText::new(format!("for '{table_name}'")).weak());
        ui.separator();
        ui.menu_button("Add analysis", |ui| {
            for spec in &menu {
                if ui.button(spec.label()).clicked() {
                    actions.push(Action::Add(spec.clone()));
                    ui.close_menu();
                }
            }
        });
        if ui
            .button("Run all")
            .on_hover_text("Re-run every analysis of this table")
            .clicked()
        {
            for (i, a) in analyses.iter().enumerate() {
                if a.table == table_index {
                    actions.push(Action::Run(i));
                }
            }
        }
    });

    ui.separator();

    let mine: Vec<usize> = analyses
        .iter()
        .enumerate()
        .filter(|(_, a)| a.table == table_index)
        .map(|(i, _)| i)
        .collect();

    if mine.is_empty() {
        ui.label(
            RichText::new(
                "No analyses yet. Use 'Add analysis' above (or the Analyses menu) to run a test.",
            )
            .weak(),
        );
    }

    for &i in &mine {
        let a = &analyses[i];
        let title = a.name();
        let headline = a
            .result
            .as_ref()
            .map(|r| r.headline())
            .unwrap_or_else(|| a.error.clone().unwrap_or_else(|| "not run yet".into()));
        ui.horizontal_wrapped(|ui| {
            let is_sel = *selected == Some(i);
            if ui
                .selectable_label(is_sel, RichText::new(&title).strong())
                .clicked()
            {
                *selected = Some(i);
            }
            if ui.small_button("Run").clicked() {
                actions.push(Action::Run(i));
            }
            if ui
                .small_button("Graph")
                .on_hover_text("Create a graph from this analysis")
                .clicked()
            {
                actions.push(Action::NewGraph(i));
            }
            if ui.small_button("Delete").clicked() {
                actions.push(Action::Delete(i));
            }
            let color = if a.error.is_some() {
                error_color(ui)
            } else {
                success_color(ui)
            };
            ui.label(RichText::new(headline).color(color).small());
        });
    }

    // Settings and results for the selected analysis.
    if let Some(idx) = *selected {
        if idx < analyses.len() && analyses[idx].table == table_index {
            ui.separator();
            let label = analyses[idx].name();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Settings").strong());
                ui.label(RichText::new(label).weak());
            });

            let mut rerun = false;
            {
                let a = &mut analyses[idx];
                match &mut a.spec {
                    AnalysisSpec::TTest {
                        kind,
                        group_a,
                        group_b,
                    } => {
                        let one_sample = *kind == TTestKind::OneSample;
                        ui.horizontal_wrapped(|ui| {
                            ComboBox::from_id_source("ttest_kind")
                                .selected_text(format!("{kind:?}"))
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    for k in [
                                        TTestKind::Unpaired,
                                        TTestKind::Welch,
                                        TTestKind::Paired,
                                        TTestKind::OneSample,
                                    ] {
                                        if ui
                                            .selectable_value(kind, k, format!("{k:?}"))
                                            .clicked()
                                        {
                                            rerun = true;
                                        }
                                    }
                                });
                            group_picker(ui, "ttest_a", "Group A", &names, group_a, &mut rerun);
                            if one_sample {
                                ui.label(RichText::new("(tested against 0)").weak());
                                *group_b = *group_a;
                            } else {
                                group_picker(
                                    ui,
                                    "ttest_b",
                                    "Group B",
                                    &names,
                                    group_b,
                                    &mut rerun,
                                );
                            }
                        });
                    }
                    AnalysisSpec::MannWhitney { group_a, group_b } => {
                        ui.horizontal_wrapped(|ui| {
                            group_picker(ui, "mw_a", "Group A", &names, group_a, &mut rerun);
                            group_picker(ui, "mw_b", "Group B", &names, group_b, &mut rerun);
                        });
                    }
                    AnalysisSpec::Wilcoxon { group_a, group_b } => {
                        ui.horizontal_wrapped(|ui| {
                            group_picker(ui, "wx_a", "Before", &names, group_a, &mut rerun);
                            group_picker(ui, "wx_b", "After", &names, group_b, &mut rerun);
                        });
                    }
                    AnalysisSpec::CurveFit { model } => {
                        ui.horizontal_wrapped(|ui| {
                            ComboBox::from_id_source("fit_model")
                                .selected_text(model.clone())
                                .width(280.0)
                                .show_ui(ui, |ui| {
                                    for m in crate::stats::regression::model_library() {
                                        if ui
                                            .selectable_value(model, m.name.to_string(), m.name)
                                            .clicked()
                                        {
                                            rerun = true;
                                        }
                                    }
                                });
                            if ui.button("Fit").clicked() {
                                rerun = true;
                            }
                        });
                    }
                    AnalysisSpec::Deming { lambda } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Lambda (variance ratio):");
                            if ui
                                .add(DragValue::new(lambda).speed(0.01).max_decimals(4))
                                .changed()
                            {
                                rerun = true;
                            }
                        });
                    }
                    AnalysisSpec::Lowess { frac } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Fraction of points used:");
                            if ui
                                .add(Slider::new(frac, 0.05..=1.0).fixed_decimals(2))
                                .changed()
                            {
                                rerun = true;
                            }
                        });
                    }
                    AnalysisSpec::Descriptive
                    | AnalysisSpec::OneWayAnova
                    | AnalysisSpec::TwoWayAnova
                    | AnalysisSpec::KruskalWallis
                    | AnalysisSpec::Friedman
                    | AnalysisSpec::ChiSquare
                    | AnalysisSpec::LinearRegression
                    | AnalysisSpec::AreaUnderCurve
                    | AnalysisSpec::CubicSpline => {
                        ui.label(RichText::new("This analysis has no options.").weak());
                    }
                }
                if ui.button("Run analysis").clicked() {
                    rerun = true;
                }
            }
            if rerun {
                actions.push(Action::Run(idx));
            }

            if let Some(result) = &analyses[idx].result {
                ui.separator();
                ui.label(RichText::new("Results").strong());
                let rows = result.summary_rows();
                ScrollArea::vertical()
                    .id_source("results_scroll")
                    .max_height((ui.available_height() - 12.0).max(120.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Grid::new("results_grid")
                            .striped(true)
                            .spacing([16.0, 3.0])
                            .show(ui, |ui| {
                                for (k, v) in rows {
                                    ui.label(k);
                                    ui.label(RichText::new(v).monospace());
                                    ui.end_row();
                                }
                            });
                    });
            } else if let Some(err) = &analyses[idx].error {
                ui.colored_label(error_color(ui), err);
            }
        }
    }

    // Apply deferred actions. Additions/runs first, deletions last (and from
    // the highest index down) so indices stay valid.
    let (mut deletes, others): (Vec<Action>, Vec<Action>) = actions
        .into_iter()
        .partition(|a| matches!(a, Action::Delete(_)));
    for action in others {
        match action {
            Action::Add(spec) => {
                analyses.push(Analysis::new(spec, table_index));
                let idx = analyses.len() - 1;
                analyses[idx].run(table);
                *selected = Some(idx);
                *revision += 1;
                changed = true;
            }
            Action::Run(i) => {
                if let Some(a) = analyses.get_mut(i) {
                    a.run(table);
                    *revision += 1;
                    changed = true;
                }
            }
            Action::NewGraph(i) => {
                if let Some(a) = analyses.get(i) {
                    let g = default_graph_for(&a.name(), flavor, i);
                    graphs.push(g);
                    *revision += 1;
                    changed = true;
                }
            }
            Action::Delete(_) => {}
        }
    }
    deletes.sort_by_key(|a| match a {
        Action::Delete(i) => std::cmp::Reverse(*i),
        _ => std::cmp::Reverse(0),
    });
    for action in deletes {
        if let Action::Delete(i) = action {
            if i < analyses.len() {
                analyses.remove(i);
                for g in graphs.iter_mut() {
                    if let Some(a) = g.analysis {
                        if a == i {
                            g.analysis = None;
                        } else if a > i {
                            g.analysis = Some(a - 1);
                        }
                    }
                }
                *selected = match *selected {
                    Some(s) if s == i => None,
                    Some(s) if s > i => Some(s - 1),
                    other => other,
                };
                *revision += 1;
                changed = true;
            }
        }
    }

    changed
}

fn group_picker(
    ui: &mut Ui,
    id: &str,
    label: &str,
    names: &[String],
    value: &mut usize,
    rerun: &mut bool,
) {
    if names.is_empty() {
        return;
    }
    if *value >= names.len() {
        *value = 0;
    }
    ComboBox::from_id_source(id)
        .selected_text(format!("{label}: {}", names[*value]))
        .width(180.0)
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_value(value, i, n).clicked() {
                    *rerun = true;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tables::{ColumnTable, GroupedTable, XyTable};

    #[test]
    fn group_names_come_from_each_table_type() {
        let c = TableInstance::Column(ColumnTable {
            name: "c".into(),
            columns: vec![vec![1.0], vec![2.0]],
            group_names: vec!["A".into(), String::new()],
        });
        assert_eq!(group_names(&c), vec!["A".to_string(), "Group 2".to_string()]);

        let g = TableInstance::Grouped(GroupedTable {
            name: "g".into(),
            row_labels: vec!["r".into()],
            group_names: vec!["X".into()],
            data: vec![vec![vec![1.0]]],
        });
        assert_eq!(group_names(&g), vec!["X".to_string()]);

        let x = TableInstance::Xy(XyTable {
            name: "x".into(),
            data: vec![(1.0, vec![1.0, 2.0, 3.0])],
        });
        assert_eq!(group_names(&x), vec!["Y1", "Y2", "Y3"]);
    }

    #[test]
    fn default_graph_matches_table_type() {
        let g = default_graph_for("fit", TableFlavor::Xy, 3);
        assert_eq!(g.plot_kind, PlotKind::Xy);
        assert_eq!(g.analysis, Some(3));
        let g = default_graph_for("anova", TableFlavor::Column, 0);
        assert_eq!(g.plot_kind, PlotKind::Bar);
        assert_eq!(g.error, ErrorKind::Sem);
        assert_eq!(g.style, SeriesStyle::Bars);
    }
}
