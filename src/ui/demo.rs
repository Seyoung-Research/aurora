//! A demo project that exercises every table type, analysis and graph.
//!
//! Used by the "Load demo project" menu item, by `--selftest`, and by the
//! end-to-end test in `ui::tests`.

use crate::analysis::{Analysis, AnalysisSpec, Graph};
use crate::data::tables::{ColumnTable, ContingencyTable, GroupedTable, XyTable};
use crate::data::{Project, TableInstance};
use crate::plot::{ErrorKind, PlotKind, SeriesStyle};
use crate::stats::comparison::TTestKind;

/// Deterministic "noise" so the demo data looks realistic but is reproducible.
fn jitter(i: usize) -> f64 {
    const J: [f64; 6] = [0.97, 1.03, 0.99, 1.02, 1.0, 0.98];
    J[i % J.len()]
}

fn add_analysis(p: &mut Project, spec: AnalysisSpec, table: usize) -> usize {
    let idx = p.analyses.len();
    p.analyses.push(Analysis::new(spec, table));
    let _ = p.run_analysis(idx);
    idx
}

#[allow(clippy::too_many_arguments)]
fn make_graph(
    name: &str,
    table: Option<usize>,
    analysis: Option<usize>,
    plot_kind: PlotKind,
    style: SeriesStyle,
    error: ErrorKind,
    x_log: bool,
    title: &str,
    x_label: &str,
    y_label: &str,
) -> Graph {
    Graph {
        name: name.into(),
        table,
        analysis,
        plot_kind,
        style,
        error,
        x_log,
        y_log: false,
        title: title.into(),
        x_label: x_label.into(),
        y_label: y_label.into(),
        show_fit: true,
        ..Default::default()
    }
}

/// Build the demo project: four tables, eight analyses and six graphs.
pub fn demo_project() -> Project {
    let mut p = Project::new("Demo project");

    // ---- 1. XY dose-response -------------------------------------------- //
    let xs = [0.01, 0.03, 0.1, 0.3, 1.0, 3.0, 10.0, 30.0, 100.0];
    let (bottom, top, log_ec50, hill) = (5.0, 100.0, 1.0, 1.0);
    let curve = |x: f64| bottom + (top - bottom) / (1.0 + 10.0_f64.powf((log_ec50 - x) * hill));
    let dose_data: Vec<(f64, Vec<f64>)> = xs
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let y = curve(x);
            (x, vec![y * jitter(i), y * jitter(i + 3), y])
        })
        .collect();
    p.add_table(TableInstance::Xy(XyTable {
        name: "Dose-response".into(),
        data: dose_data,
    }));

    // ---- 2. Column table: three treatment groups ------------------------- //
    p.add_table(TableInstance::Column(ColumnTable {
        name: "Treatment groups".into(),
        columns: vec![
            vec![1.2, 1.4, 1.1, 1.3, 1.25, 1.35],
            vec![2.1, 2.4, 2.0, 2.2, 2.3, 2.15],
            vec![4.2, 4.6, 4.0, 4.4, 4.3, 4.5],
        ],
        group_names: vec!["Control".into(), "Low dose".into(), "High dose".into()],
    }));

    // ---- 3. Grouped table: two factors ---------------------------------- //
    p.add_table(TableInstance::Grouped(GroupedTable {
        name: "Time course".into(),
        row_labels: vec!["Day 1".into(), "Day 2".into(), "Day 3".into()],
        group_names: vec!["Vehicle".into(), "Drug A".into(), "Drug B".into()],
        data: vec![
            vec![
                vec![1.0, 1.2, 0.9, 1.1],
                vec![1.6, 1.8, 1.5, 1.7],
                vec![1.1, 1.3, 1.0, 1.2],
            ],
            vec![
                vec![2.1, 2.3, 2.0, 2.2],
                vec![3.9, 4.2, 3.7, 4.0],
                vec![2.4, 2.6, 2.2, 2.5],
            ],
            vec![
                vec![3.4, 3.6, 3.2, 3.5],
                vec![6.1, 6.4, 5.9, 6.2],
                vec![3.8, 4.1, 3.6, 3.9],
            ],
        ],
    }));

    // ---- 4. Contingency table ------------------------------------------- //
    p.add_table(TableInstance::Contingency(ContingencyTable {
        name: "Outcomes".into(),
        row_labels: vec!["Treated".into(), "Control".into()],
        col_labels: vec!["Responded".into(), "Did not respond".into()],
        data: vec![vec![72, 28], vec![41, 59]],
    }));

    // ---- Analyses -------------------------------------------------------- //
    let fit = add_analysis(
        &mut p,
        AnalysisSpec::CurveFit {
            model: "Sigmoidal dose-response (4PL)".into(),
        },
        0,
    );
    add_analysis(&mut p, AnalysisSpec::Descriptive, 1);
    let anova = add_analysis(&mut p, AnalysisSpec::OneWayAnova, 1);
    add_analysis(
        &mut p,
        AnalysisSpec::TTest {
            kind: TTestKind::Unpaired,
            group_a: 0,
            group_b: 2,
        },
        1,
    );
    add_analysis(&mut p, AnalysisSpec::KruskalWallis, 1);
    let twoway = add_analysis(&mut p, AnalysisSpec::TwoWayAnova, 2);
    let chisq = add_analysis(&mut p, AnalysisSpec::ChiSquare, 3);
    add_analysis(&mut p, AnalysisSpec::AreaUnderCurve, 0);

    // ---- Graphs ---------------------------------------------------------- //
    p.graphs.push(make_graph(
        "Dose-response",
        Some(0),
        Some(fit),
        PlotKind::Xy,
        SeriesStyle::LinesAndPoints,
        ErrorKind::Sem,
        true,
        "Dose-response curve",
        "Concentration (nM)",
        "Response (AU)",
    ));
    p.graphs.push(make_graph(
        "Bars with SEM",
        Some(1),
        Some(anova),
        PlotKind::Bar,
        SeriesStyle::Bars,
        ErrorKind::Sem,
        false,
        "Treatment effect (one-way ANOVA)",
        "Group",
        "Response (AU)",
    ));
    p.graphs.push(make_graph(
        "Column scatter",
        Some(1),
        None,
        PlotKind::Column,
        SeriesStyle::Columns,
        ErrorKind::Sem,
        false,
        "Individual values",
        "Group",
        "Response (AU)",
    ));
    p.graphs.push(make_graph(
        "Box plot",
        Some(1),
        None,
        PlotKind::BoxAndWhisker,
        SeriesStyle::Boxes,
        ErrorKind::None,
        false,
        "Distribution of each group",
        "Group",
        "Response (AU)",
    ));
    p.graphs.push(make_graph(
        "Two-way grouped bars",
        Some(2),
        Some(twoway),
        PlotKind::GroupedBar,
        SeriesStyle::Bars,
        ErrorKind::Sem,
        false,
        "Time course by treatment",
        "Day",
        "Concentration (uM)",
    ));
    p.graphs.push(make_graph(
        "Contingency counts",
        Some(3),
        Some(chisq),
        PlotKind::GroupedBar,
        SeriesStyle::Bars,
        ErrorKind::None,
        false,
        "Outcomes by group",
        "Group",
        "Count",
    ));

    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_every_table_type() {
        let p = demo_project();
        let flavors: Vec<_> = p.tables.iter().map(|t| t.flavor()).collect();
        assert_eq!(
            flavors,
            vec![
                crate::data::TableFlavor::Xy,
                crate::data::TableFlavor::Column,
                crate::data::TableFlavor::Grouped,
                crate::data::TableFlavor::Contingency,
            ]
        );
    }

    #[test]
    fn demo_analyses_all_succeed() {
        let p = demo_project();
        assert_eq!(p.analyses.len(), 8);
        for (i, a) in p.analyses.iter().enumerate() {
            assert!(a.error.is_none(), "analysis {i} ({}) failed: {:?}", a.spec.label(), a.error);
            assert!(a.result.is_some(), "analysis {i} produced no result");
        }
    }

    #[test]
    fn demo_graphs_all_render() {
        let p = demo_project();
        assert_eq!(p.graphs.len(), 6);
        for i in 0..p.graphs.len() {
            let chart = p
                .chart_for_graph(i)
                .unwrap_or_else(|| panic!("graph {i} has no chart"));
            assert!(!chart.series.is_empty(), "graph {i} has no data");
            let img = crate::plot::render::render(&chart, 400, 300).expect("render");
            assert_eq!(img.rgb.len(), 400 * 300 * 3);
        }
    }

    #[test]
    fn demo_survives_a_json_round_trip() {
        let p = demo_project();
        let json = serde_json::to_string(&p).expect("serialize");
        let back: Project = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.tables.len(), p.tables.len());
        assert_eq!(back.analyses.len(), p.analyses.len());
        assert_eq!(back.graphs.len(), p.graphs.len());
    }
}
