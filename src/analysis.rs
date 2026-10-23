//! Analyses: what to compute for a table, the results, and the graphs built
//! from them.
//!
//! An [`Analysis`] is a saved (spec, result) pair that belongs to a
//! [`crate::data::Project`]. A [`Graph`] references a table and/or an analysis
//! and describes how to draw it.

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::data::tables::{ColumnTable, XyTable};
use crate::data::{TableFlavor, TableInstance};
use crate::plot::{
    column_chart, grouped_chart, xy_chart, xy_chart_by_replicate, Chart, ErrorKind, Marker,
    PlotKind, Series, SeriesStyle,
};
use crate::stats::comparison::{
    chi_square_independence, friedman, kruskal_wallis, mann_whitney_u, one_way_anova,
    t_test_paired, t_test_unpaired, two_way_anova, wilcoxon_signed_rank, AnovaResult,
    ChiSquareResult, FriedmanResult, KruskalWallisResult, MannWhitneyResult, TTestKind,
    TTestResult, TwoWayAnovaResult, WilcoxonResult,
};
use crate::stats::descriptive::{calculate_descriptive, DescriptiveStats};
use crate::stats::regression::{
    deming_regression, fit_curve, lowess, model_library, simple_linear_regression,
    trapezoidal_auc, CurveFitResult, LinearRegressionResult,
};

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

/// What an analysis computes.
///
/// An analysis is a saved specification of a statistical test or summary; it is
/// paired with an [`AnalysisResult`] when run. Which specifications are offered
/// for a given table flavour is decided by [`AnalysisSpec::menu_for`], and which
/// ones are legal are decided by [`AnalysisSpec::applicable_to`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AnalysisSpec {
    /// Descriptive statistics for every column / row of groups.
    Descriptive,
    /// Two-sample t-test comparing two groups of a column table.
    ///
    /// The kind of test (unpaired / Welch / paired / one-sample) and the two
    /// groups to compare are part of the specification.
    TTest {
        /// Which t-test variant to run.
        kind: TTestKind,
        /// Index of the first group (column) to compare.
        group_a: usize,
        /// Index of the second group (column) to compare.
        group_b: usize,
    },
    /// One-way analysis of variance across groups.
    OneWayAnova,
    /// Two-way analysis of variance with replication, across two factors.
    TwoWayAnova,
    /// Mann-Whitney U rank test comparing two groups.
    ///
    /// The two groups are part of the specification.
    MannWhitney {
        /// Index of the first group (column) to compare.
        group_a: usize,
        /// Index of the second group (column) to compare.
        group_b: usize,
    },
    /// Wilcoxon signed-rank test for paired data.
    ///
    /// The two groups are part of the specification.
    Wilcoxon {
        /// Index of the first group (column) to compare.
        group_a: usize,
        /// Index of the second group (column) to compare.
        group_b: usize,
    },
    /// Kruskal-Wallis rank-based one-way analysis of variance.
    KruskalWallis,
    /// Friedman test for repeated-measures (paired) data.
    Friedman,
    /// Chi-square test of independence for a contingency table.
    ChiSquare,
    /// Simple linear regression on an XY table.
    LinearRegression,
    /// Deming regression (errors-in-variables) with the given lambda.
    Deming {
        /// Ratio of variances of the errors in x and y (`1.0` for equal errors).
        lambda: f64,
    },
    /// Fit a named non-linear model to an XY table.
    CurveFit {
        /// Name of the model from the regression model library.
        model: String,
    },
    /// Area under the curve of an XY table (trapezoidal rule).
    AreaUnderCurve,
    /// LOWESS smoothing of an XY table.
    ///
    /// The smoothing fraction is part of the specification.
    Lowess {
        /// Smoothing fraction (clamped to [0.05, 1.0]).
        frac: f64,
    },
    /// Cubic spline interpolation of an XY table.
    CubicSpline,
}

impl AnalysisSpec {
    /// Short name shown in menus and the project tree.
    pub fn label(&self) -> String {
        match self {
            AnalysisSpec::Descriptive => "Descriptive statistics".into(),
            AnalysisSpec::TTest {
                kind,
                group_a,
                group_b,
            } => {
                let k = match kind {
                    TTestKind::Unpaired => "Unpaired t test",
                    TTestKind::Welch => "Welch t test",
                    TTestKind::Paired => "Paired t test",
                    TTestKind::OneSample => "One-sample t test",
                };
                format!("{k} (col {} vs {})", group_a + 1, group_b + 1)
            }
            AnalysisSpec::OneWayAnova => "One-way ANOVA".into(),
            AnalysisSpec::TwoWayAnova => "Two-way ANOVA".into(),
            AnalysisSpec::MannWhitney { group_a, group_b } => {
                format!("Mann-Whitney (col {} vs {})", group_a + 1, group_b + 1)
            }
            AnalysisSpec::Wilcoxon { group_a, group_b } => {
                format!("Wilcoxon matched pairs (col {} vs {})", group_a + 1, group_b + 1)
            }
            AnalysisSpec::KruskalWallis => "Kruskal-Wallis test".into(),
            AnalysisSpec::Friedman => "Friedman test".into(),
            AnalysisSpec::ChiSquare => "Chi-square test".into(),
            AnalysisSpec::LinearRegression => "Simple linear regression".into(),
            AnalysisSpec::Deming { lambda } => format!("Deming regression (lambda={lambda:.2})"),
            AnalysisSpec::CurveFit { model } => format!("Nonlinear fit: {model}"),
            AnalysisSpec::AreaUnderCurve => "Area under the curve".into(),
            AnalysisSpec::Lowess { frac } => format!("LOWESS smoothing (frac={frac:.2})"),
            AnalysisSpec::CubicSpline => "Cubic spline interpolation".into(),
        }
    }

    /// Tables this analysis can be run on.
    pub fn applicable_to(&self, flavor: TableFlavor) -> bool {
        match self {
            AnalysisSpec::Descriptive => {
                matches!(flavor, TableFlavor::Column | TableFlavor::Xy | TableFlavor::Grouped)
            }
            AnalysisSpec::TTest { kind, .. } => match kind {
                TTestKind::OneSample => flavor == TableFlavor::Column,
                TTestKind::Paired => flavor == TableFlavor::Column,
                _ => flavor == TableFlavor::Column,
            },
            AnalysisSpec::OneWayAnova => flavor == TableFlavor::Column,
            AnalysisSpec::TwoWayAnova => flavor == TableFlavor::Grouped,
            AnalysisSpec::MannWhitney { .. }
            | AnalysisSpec::Wilcoxon { .. }
            | AnalysisSpec::KruskalWallis
            | AnalysisSpec::Friedman => flavor == TableFlavor::Column,
            AnalysisSpec::ChiSquare => flavor == TableFlavor::Contingency,
            AnalysisSpec::LinearRegression
            | AnalysisSpec::Deming { .. }
            | AnalysisSpec::CurveFit { .. }
            | AnalysisSpec::AreaUnderCurve
            | AnalysisSpec::Lowess { .. }
            | AnalysisSpec::CubicSpline => flavor == TableFlavor::Xy,
        }
    }

    /// The practical list of analyses offered for a table type.
    pub fn menu_for(flavor: TableFlavor, n_groups: usize) -> Vec<AnalysisSpec> {
        let mut out = Vec::new();
        let groups = n_groups.max(2);
        match flavor {
            TableFlavor::Column => {
                out.push(AnalysisSpec::Descriptive);
                out.push(AnalysisSpec::TTest {
                    kind: TTestKind::Unpaired,
                    group_a: 0,
                    group_b: 1,
                });
                out.push(AnalysisSpec::TTest {
                    kind: TTestKind::Welch,
                    group_a: 0,
                    group_b: 1,
                });
                out.push(AnalysisSpec::TTest {
                    kind: TTestKind::Paired,
                    group_a: 0,
                    group_b: 1,
                });
                out.push(AnalysisSpec::TTest {
                    kind: TTestKind::OneSample,
                    group_a: 0,
                    group_b: 0,
                });
                out.push(AnalysisSpec::OneWayAnova);
                out.push(AnalysisSpec::MannWhitney {
                    group_a: 0,
                    group_b: 1,
                });
                out.push(AnalysisSpec::Wilcoxon {
                    group_a: 0,
                    group_b: 1,
                });
                out.push(AnalysisSpec::KruskalWallis);
                out.push(AnalysisSpec::Friedman);
                let _ = groups;
            }
            TableFlavor::Grouped => {
                out.push(AnalysisSpec::Descriptive);
                out.push(AnalysisSpec::TwoWayAnova);
            }
            TableFlavor::Contingency => {
                out.push(AnalysisSpec::ChiSquare);
            }
            TableFlavor::Xy => {
                out.push(AnalysisSpec::Descriptive);
                out.push(AnalysisSpec::LinearRegression);
                out.push(AnalysisSpec::Deming { lambda: 1.0 });
                for m in model_library() {
                    out.push(AnalysisSpec::CurveFit {
                        model: m.name.to_string(),
                    });
                }
                out.push(AnalysisSpec::AreaUnderCurve);
                out.push(AnalysisSpec::Lowess { frac: 0.3 });
                out.push(AnalysisSpec::CubicSpline);
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// A named summary statistic: which table column / X value it belongs to, plus
/// the statistics computed for it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NamedStats {
    /// Name of the group or of the X value the statistics belong to.
    pub name: String,
    /// The statistics themselves.
    pub stats: DescriptiveStats,
}

/// The outcome of running an [`AnalysisSpec`] on a table.
///
/// The variant encodes which kind of result is present; accessors such as
/// [`AnalysisResult::summary_rows`] and [`AnalysisResult::headline`] turn a
/// result into display-ready text.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum AnalysisResult {
    /// Descriptive statistics for every group / X value.
    Descriptive(Vec<NamedStats>),
    /// Outcome of a t-test.
    TTest(TTestResult),
    /// Outcome of a one-way ANOVA (with pairwise comparisons).
    Anova(AnovaResult),
    /// Outcome of a two-way ANOVA with replication.
    TwoWay(TwoWayAnovaResult),
    /// Outcome of a Mann-Whitney U test.
    MannWhitney(MannWhitneyResult),
    /// Outcome of a Wilcoxon signed-rank test.
    Wilcoxon(WilcoxonResult),
    /// Outcome of a Kruskal-Wallis test.
    KruskalWallis(KruskalWallisResult),
    /// Outcome of a Friedman test.
    Friedman(FriedmanResult),
    /// Outcome of a chi-square test of independence.
    ChiSquare(ChiSquareResult),
    /// Outcome of a linear regression.
    LinearRegression(LinearRegressionResult),
    /// Outcome of a non-linear curve fit.
    Curve(CurveFitResult),
    /// Named scalars (e.g. AUC) or any small numeric summary.
    Summary(Vec<(String, f64)>),
}

/// Format a float for display: scientific notation for extreme magnitudes and
/// `n/a` for non-finite values.
fn f(v: f64) -> String {
    if !v.is_finite() {
        return "n/a".to_string();
    }
    let a = v.abs();
    if a != 0.0 && (a < 1e-4 || a >= 1e6) {
        format!("{v:.4e}")
    } else {
        format!("{v:.4}")
    }
}

/// Format a p-value for display, marking values below 0.0001 as `< 0.0001 (x)`.
fn p_string(p: f64) -> String {
    if !p.is_finite() {
        return "n/a".to_string();
    }
    if p < 1e-5 {
        format!("< 0.0001 ({})", f(p))
    } else {
        format!("{p:.4}")
    }
}

impl AnalysisResult {
    /// Human-readable `(label, value)` rows for the results panel.
    pub fn summary_rows(&self) -> Vec<(String, String)> {
        use AnalysisResult::*;
        let mut rows: Vec<(String, String)> = Vec::new();
        match self {
            Descriptive(list) => {
                for ns in list {
                    let s = &ns.stats;
                    rows.push((format!("{}: n", ns.name), s.n.to_string()));
                    rows.push((format!("{}: mean", ns.name), f(s.mean)));
                    rows.push((format!("{}: SD", ns.name), f(s.std_dev)));
                    rows.push((format!("{}: SEM", ns.name), f(s.std_error)));
                    rows.push((format!("{}: median", ns.name), f(s.median)));
                    rows.push((
                        format!("{}: 95% CI", ns.name),
                        format!("{} to {}", f(s.ci95_lower), f(s.ci95_upper)),
                    ));
                    rows.push((format!("{}: min..max", ns.name), format!("{} .. {}", f(s.min), f(s.max))));
                }
            }
            TTest(r) => {
                rows.push(("Test".into(), format!("{:?}", r.kind)));
                rows.push(("t".into(), f(r.t)));
                rows.push(("df".into(), f(r.df)));
                rows.push(("p (two-tailed)".into(), p_string(r.p_value)));
                rows.push(("Mean A".into(), f(r.mean_a)));
                rows.push(("Mean B".into(), f(r.mean_b)));
                rows.push(("Difference".into(), f(r.mean_diff)));
                rows.push((
                    "95% CI of difference".into(),
                    format!("{} to {}", f(r.ci95_diff.0), f(r.ci95_diff.1)),
                ));
                rows.push(("Effect size".into(), f(r.effect_size)));
                rows.push(("n".into(), format!("{} vs {}", r.n_a, r.n_b)));
            }
            Anova(r) => {
                rows.push(("F".into(), f(r.f)));
                rows.push((
                    "df".into(),
                    format!("{}, {}", f(r.df_between), f(r.df_within)),
                ));
                rows.push(("p".into(), p_string(r.p_value)));
                rows.push(("SS between".into(), f(r.ss_between)));
                rows.push(("SS within".into(), f(r.ss_within)));
                rows.push(("MS within".into(), f(r.ms_within)));
                rows.push(("Eta squared".into(), f(r.eta_squared)));
                for (i, (m, n)) in r.group_means.iter().zip(&r.group_ns).enumerate() {
                    rows.push((format!("Group {} mean (n={})", i + 1, n), f(*m)));
                }
                for p in &r.pairwise {
                    rows.push((
                        format!("Group {} vs {} (Bonferroni)", p.group_a + 1, p.group_b + 1),
                        format!("diff {} p={}", f(p.mean_diff), p_string(p.p_adjusted)),
                    ));
                }
            }
            TwoWay(r) => {
                rows.push(("F (factor A)".into(), f(r.f_a)));
                rows.push(("p (factor A)".into(), p_string(r.p_a)));
                rows.push(("F (factor B)".into(), f(r.f_b)));
                rows.push(("p (factor B)".into(), p_string(r.p_b)));
                rows.push(("F (interaction)".into(), f(r.f_interaction)));
                rows.push(("p (interaction)".into(), p_string(r.p_interaction)));
                rows.push((
                    "df".into(),
                    format!(
                        "A {}, B {}, AB {}, error {}",
                        f(r.df_a),
                        f(r.df_b),
                        f(r.df_interaction),
                        f(r.df_error)
                    ),
                ));
                rows.push(("MS error".into(), f(r.ms_error)));
            }
            MannWhitney(r) => {
                rows.push(("U".into(), f(r.u_statistic)));
                rows.push(("z".into(), f(r.z)));
                rows.push(("p (two-tailed)".into(), p_string(r.p_value)));
                rows.push(("Median A".into(), f(r.median_a)));
                rows.push(("Median B".into(), f(r.median_b)));
                rows.push(("n".into(), format!("{} vs {}", r.n_a, r.n_b)));
            }
            Wilcoxon(r) => {
                rows.push(("W+".into(), f(r.w_plus)));
                rows.push(("W-".into(), f(r.w_minus)));
                rows.push(("z".into(), f(r.z)));
                rows.push(("p (two-tailed)".into(), p_string(r.p_value)));
                rows.push(("Pairs used".into(), r.n_effective.to_string()));
            }
            KruskalWallis(r) => {
                rows.push(("H".into(), f(r.h)));
                rows.push(("df".into(), f(r.df)));
                rows.push(("p".into(), p_string(r.p_value)));
            }
            Friedman(r) => {
                rows.push(("Chi-square".into(), f(r.chi_square)));
                rows.push(("df".into(), f(r.df)));
                rows.push(("p".into(), p_string(r.p_value)));
            }
            ChiSquare(r) => {
                rows.push(("Chi-square".into(), f(r.chi_square)));
                rows.push(("df".into(), f(r.df)));
                rows.push(("p".into(), p_string(r.p_value)));
                rows.push(("Cramer's V".into(), f(r.cramers_v)));
                for (i, row) in r.expected.iter().enumerate() {
                    let vals = row.iter().map(|v| f(*v)).collect::<Vec<_>>().join(", ");
                    rows.push((format!("Expected row {}", i + 1), vals));
                }
            }
            LinearRegression(r) => {
                rows.push(("Slope".into(), f(r.slope)));
                rows.push(("Slope SE".into(), f(r.slope_se)));
                rows.push((
                    "Slope 95% CI".into(),
                    format!("{} to {}", f(r.slope_ci95.0), f(r.slope_ci95.1)),
                ));
                rows.push(("Slope p".into(), p_string(r.slope_p)));
                rows.push(("Intercept".into(), f(r.intercept)));
                rows.push(("Intercept SE".into(), f(r.intercept_se)));
                rows.push(("R squared".into(), f(r.r_squared)));
                rows.push(("n".into(), r.n.to_string()));
            }
            Curve(r) => {
                rows.push(("Model".into(), r.model.clone()));
                for (name, v) in r.param_names.iter().zip(&r.params) {
                    rows.push((name.clone(), f(*v)));
                }
                for (name, v) in r.param_names.iter().zip(&r.param_std_errors) {
                    rows.push((format!("{name} SE"), f(*v)));
                }
                rows.push(("R squared".into(), f(r.r_squared)));
                rows.push(("RMSE".into(), f(r.rmse)));
                rows.push((
                    "Converged".into(),
                    if r.converged { "yes" } else { "no" }.to_string(),
                ));
            }
            Summary(list) => {
                for (k, v) in list {
                    rows.push((k.clone(), f(*v)));
                }
            }
        }
        rows
    }

    /// One-line headline for the project tree.
    pub fn headline(&self) -> String {
        match self {
            AnalysisResult::TTest(r) => format!("p = {}", p_string(r.p_value)),
            AnalysisResult::Anova(r) => format!("F = {}, p = {}", f(r.f), p_string(r.p_value)),
            AnalysisResult::TwoWay(r) => format!("p(A) = {}, p(B) = {}", p_string(r.p_a), p_string(r.p_b)),
            AnalysisResult::MannWhitney(r) => format!("U = {}, p = {}", f(r.u_statistic), p_string(r.p_value)),
            AnalysisResult::Wilcoxon(r) => format!("p = {}", p_string(r.p_value)),
            AnalysisResult::KruskalWallis(r) => format!("H = {}, p = {}", f(r.h), p_string(r.p_value)),
            AnalysisResult::Friedman(r) => format!("chi2 = {}, p = {}", f(r.chi_square), p_string(r.p_value)),
            AnalysisResult::ChiSquare(r) => format!("chi2 = {}, p = {}", f(r.chi_square), p_string(r.p_value)),
            AnalysisResult::LinearRegression(r) => format!(
                "y = {}x + {}, R2 = {}",
                f(r.slope),
                f(r.intercept),
                f(r.r_squared)
            ),
            AnalysisResult::Curve(r) => format!("R2 = {}", f(r.r_squared)),
            AnalysisResult::Descriptive(l) => format!("{} group(s)", l.len()),
            AnalysisResult::Summary(_) => "computed".into(),
        }
    }

    /// Fitted curve/line, if this result has one, for graph overlays.
    pub fn fit_curve(&self) -> Option<(String, Vec<f64>, f64)> {
        match self {
            AnalysisResult::Curve(c) => Some((c.model.clone(), c.params.clone(), 1.0)),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Analysis + graph model
// ---------------------------------------------------------------------------

/// A single analysis of a project: which test to run on which table, and the
/// outcome of the last run (if any).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Analysis {
    /// The statistical test / summary that is (or will be) run.
    pub spec: AnalysisSpec,
    /// Index into `Project::tables` the analysis was set up for.
    pub table: usize,
    /// Result of the last run, if it succeeded.
    pub result: Option<AnalysisResult>,
    /// Set when the last run failed; holds the error message.
    pub error: Option<String>,
}

impl Analysis {
    /// Create a new, not-yet-run analysis of the given spec on the given table.
    pub fn new(spec: AnalysisSpec, table: usize) -> Self {
        Self {
            spec,
            table,
            result: None,
            error: None,
        }
    }

    /// Short human-readable name of this analysis (its spec's label).
    pub fn name(&self) -> String {
        self.spec.label()
    }

    /// Run the analysis against the given table and store the result or error.
    ///
    /// On success the previous result (and error) is replaced; on failure both
    /// are cleared and the error is kept for display.
    pub fn run(&mut self, table: &TableInstance) {
        match run(&self.spec, table) {
            Ok(r) => {
                self.result = Some(r);
                self.error = None;
            }
            Err(e) => {
                self.result = None;
                self.error = Some(e.to_string());
            }
        }
    }
}

/// How to draw a graph.
///
/// A graph binds a table (or an analysis) and describes how the chart should
/// look: which plot kind, series style, error bars, scales and whether a fitted
/// curve should be overlaid.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Graph {
    /// Display name of the graph.
    pub name: String,
    /// Index into `Project::tables` (`None` uses the analysis's table).
    pub table: Option<usize>,
    /// Index into `Project::analyses`; when set, the fitted curve is drawn if
    /// `show_fit` is enabled.
    pub analysis: Option<usize>,
    /// Plot kind driving the layout of the x axis.
    pub plot_kind: PlotKind,
    /// How series are drawn (lines, markers, bars, boxes, ...).
    pub style: SeriesStyle,
    /// Error bar to derive from replicates, if any.
    pub error: ErrorKind,
    /// Whether the x axis uses a base-10 logarithm.
    pub x_log: bool,
    /// Whether the y axis uses a base-10 logarithm.
    pub y_log: bool,
    /// Title of the chart; falls back to the graph's name.
    pub title: String,
    /// Label of the x axis; falls back to the table's name.
    pub x_label: String,
    /// Label of the y axis; falls back to the table's name.
    pub y_label: String,
    /// Draw the fitted curve from the referenced analysis.
    pub show_fit: bool,
    /// Draw a legend when the chart has several meaningful series.
    #[serde(default = "graph_default_true")]
    pub legend: bool,
    /// Draw one series per replicate instead of a summary series (XY tables).
    #[serde(default)]
    pub by_replicate: bool,
    /// Horizontal jitter (0.0-0.5) of the individual points in scatter plots.
    #[serde(default = "graph_default_jitter")]
    pub jitter: f64,
    /// Canvas size used when exporting this graph as a PNG.
    #[serde(default = "graph_default_width")]
    pub width: u32,
    #[serde(default = "graph_default_height")]
    pub height: u32,
}

/// Serde default for [`Graph::legend`]: legends are on by default.
fn graph_default_true() -> bool {
    true
}

/// Serde default for [`Graph::jitter`]: horizontal scatter jitter of 0.25.
fn graph_default_jitter() -> f64 {
    0.25
}

/// Serde default for [`Graph::width`]: 1000 px canvas width.
fn graph_default_width() -> u32 {
    1000
}

/// Serde default for [`Graph::height`]: 700 px canvas height.
fn graph_default_height() -> u32 {
    700
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            name: "Graph".into(),
            table: None,
            analysis: None,
            plot_kind: PlotKind::Xy,
            style: SeriesStyle::LinesAndPoints,
            error: ErrorKind::Sem,
            x_log: false,
            y_log: false,
            title: String::new(),
            x_label: String::new(),
            y_label: String::new(),
            show_fit: true,
            legend: graph_default_true(),
            by_replicate: false,
            jitter: graph_default_jitter(),
            width: graph_default_width(),
            height: graph_default_height(),
        }
    }
}

// ---------------------------------------------------------------------------
// Running analyses
// ---------------------------------------------------------------------------

/// Return the column table, or an error when the table is not a column table.
fn require_column(table: &TableInstance) -> Result<&ColumnTable> {
    match table {
        TableInstance::Column(t) => Ok(t),
        _ => bail!("This analysis needs a column table"),
    }
}

/// Return the XY table, or an error when the table is not an XY table.
fn require_xy(table: &TableInstance) -> Result<&XyTable> {
    match table {
        TableInstance::Xy(t) => Ok(t),
        _ => bail!("This analysis needs an XY table"),
    }
}

/// Return the i-th column of a column table (index in error messages is 1-based).
fn column(t: &ColumnTable, i: usize) -> Result<&Vec<f64>> {
    t.columns
        .get(i)
        .ok_or_else(|| anyhow!("Column {} does not exist", i + 1))
}

/// Group display name for column `i`, falling back to `Group N` when unnamed.
fn group_name(t: &ColumnTable, i: usize) -> String {
    t.group_names
        .get(i)
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Group {}", i + 1))
}

/// Mean Y for each X of an XY table, plus the raw replicates.
fn xy_pairs(t: &XyTable) -> Vec<(f64, f64, Vec<f64>)> {
    t.data
        .iter()
        .filter(|(_, reps)| !reps.is_empty())
        .map(|(x, reps)| {
            let mean = reps.iter().sum::<f64>() / reps.len() as f64;
            (*x, mean, reps.clone())
        })
        .collect()
}

/// Run an analysis against a table.
///
/// Checks that the spec is applicable to the table's flavor and then executes
/// the corresponding statistical routine. The result of the run is expected to
/// be turned into a displayable [`AnalysisResult`] by the caller.
///
/// # Errors
///
/// Returns an error when the spec is not applicable to the table type, or when
/// the underlying statistic cannot be computed (degenerate data, unknown model,
/// ...).
pub fn run(spec: &AnalysisSpec, table: &TableInstance) -> Result<AnalysisResult> {
    if !spec.applicable_to(table.flavor()) {
        bail!("{} cannot be run on a {} table", spec.label(), table.flavor().label());
    }
    match spec {
        AnalysisSpec::Descriptive => match table {
            TableInstance::Column(t) => {
                let list = t
                    .columns
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        calculate_descriptive(c).map(|stats| NamedStats {
                            name: group_name(t, i),
                            stats,
                        })
                    })
                    .collect();
                Ok(AnalysisResult::Descriptive(list))
            }
            TableInstance::Xy(t) => {
                let list = t
                    .data
                    .iter()
                    .filter_map(|(x, reps)| {
                        calculate_descriptive(reps).map(|stats| NamedStats {
                            name: format!("X = {x}"),
                            stats,
                        })
                    })
                    .collect();
                Ok(AnalysisResult::Descriptive(list))
            }
            TableInstance::Grouped(t) => {
                let mut list = Vec::new();
                for (r, row) in t.data.iter().enumerate() {
                    for (g, cell) in row.iter().enumerate() {
                        if let Some(stats) = calculate_descriptive(cell) {
                            let row_label = t
                                .row_labels
                                .get(r)
                                .cloned()
                                .unwrap_or_else(|| format!("Row {}", r + 1));
                            let group = t
                                .group_names
                                .get(g)
                                .cloned()
                                .unwrap_or_else(|| format!("Group {}", g + 1));
                            list.push(NamedStats {
                                name: format!("{row_label} / {group}"),
                                stats,
                            });
                        }
                    }
                }
                Ok(AnalysisResult::Descriptive(list))
            }
            _ => bail!("Descriptive statistics need a column, XY or grouped table"),
        },
        AnalysisSpec::TTest {
            kind,
            group_a,
            group_b,
        } => {
            let t = require_column(table)?;
            let a = column(t, *group_a)?;
            let b = column(t, *group_b)?;
            let r = match kind {
                TTestKind::Unpaired => t_test_unpaired(a, b, false)?,
                TTestKind::Welch => t_test_unpaired(a, b, true)?,
                TTestKind::Paired => t_test_paired(a, b)?,
                TTestKind::OneSample => {
                    let mut r = crate::stats::comparison::t_test_one_sample(a, 0.0)?;
                    r.mean_b = 0.0;
                    r
                }
            };
            Ok(AnalysisResult::TTest(r))
        }
        AnalysisSpec::OneWayAnova => {
            let t = require_column(table)?;
            Ok(AnalysisResult::Anova(one_way_anova(&t.columns)?))
        }
        AnalysisSpec::TwoWayAnova => match table {
            TableInstance::Grouped(t) => Ok(AnalysisResult::TwoWay(two_way_anova(&t.data)?)),
            _ => bail!("Two-way ANOVA needs a grouped table"),
        },
        AnalysisSpec::MannWhitney { group_a, group_b } => {
            let t = require_column(table)?;
            Ok(AnalysisResult::MannWhitney(mann_whitney_u(
                column(t, *group_a)?,
                column(t, *group_b)?,
            )?))
        }
        AnalysisSpec::Wilcoxon { group_a, group_b } => {
            let t = require_column(table)?;
            Ok(AnalysisResult::Wilcoxon(wilcoxon_signed_rank(
                column(t, *group_a)?,
                column(t, *group_b)?,
            )?))
        }
        AnalysisSpec::KruskalWallis => {
            let t = require_column(table)?;
            Ok(AnalysisResult::KruskalWallis(kruskal_wallis(&t.columns)?))
        }
        AnalysisSpec::Friedman => {
            let t = require_column(table)?;
            let n_rows = t.columns.iter().map(|c| c.len()).min().unwrap_or(0);
            if n_rows == 0 || t.columns.len() < 2 {
                bail!("Friedman needs at least 2 treatments with paired rows");
            }
            let blocks: Vec<Vec<f64>> = (0..n_rows)
                .map(|r| t.columns.iter().map(|c| c[r]).collect())
                .collect();
            Ok(AnalysisResult::Friedman(friedman(&blocks)?))
        }
        AnalysisSpec::ChiSquare => match table {
            TableInstance::Contingency(t) => {
                Ok(AnalysisResult::ChiSquare(chi_square_independence(&t.data)?))
            }
            _ => bail!("Chi-square needs a contingency table"),
        },
        AnalysisSpec::LinearRegression => {
            let t = require_xy(table)?;
            let pairs = xy_pairs(t);
            let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let y: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            Ok(AnalysisResult::LinearRegression(simple_linear_regression(
                &x, &y,
            )?))
        }
        AnalysisSpec::Deming { lambda } => {
            let t = require_xy(table)?;
            let pairs = xy_pairs(t);
            let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let y: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            Ok(AnalysisResult::LinearRegression(deming_regression(
                &x, &y, *lambda,
            )?))
        }
        AnalysisSpec::CurveFit { model } => {
            let t = require_xy(table)?;
            let pairs = xy_pairs(t);
            let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let y: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            Ok(AnalysisResult::Curve(fit_curve(model, &x, &y)?))
        }
        AnalysisSpec::AreaUnderCurve => {
            let t = require_xy(table)?;
            let pairs = xy_pairs(t);
            let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let y: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            let mut rows = vec![("AUC (trapezoid)".to_string(), trapezoidal_auc(&x, &y)?)];
            // Also report the baseline-subtracted area when a baseline is
            // detectable (the minimum value).
            if let Some(min) = y.iter().cloned().reduce(f64::min) {
                let shifted: Vec<f64> = y.iter().map(|v| v - min).collect();
                rows.push((
                    "AUC above minimum".to_string(),
                    trapezoidal_auc(&x, &shifted)?,
                ));
                rows.push(("Baseline (min Y)".to_string(), min));
            }
            Ok(AnalysisResult::Summary(rows))
        }
        AnalysisSpec::Lowess { frac } => {
            let t = require_xy(table)?;
            let pairs = xy_pairs(t);
            let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let y: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            let smoothed = lowess(&x, &y, *frac, 2)?;
            let rows: Vec<(String, f64)> = x
                .iter()
                .zip(smoothed.iter())
                .map(|(xi, s)| (format!("smoothed at x = {xi}"), *s))
                .collect();
            Ok(AnalysisResult::Summary(rows))
        }
        AnalysisSpec::CubicSpline => {
            let t = require_xy(table)?;
            let pairs = xy_pairs(t);
            let x: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let y: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            let mut sorted: Vec<(f64, f64)> = x.iter().cloned().zip(y.iter().cloned()).collect();
            sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let sx: Vec<f64> = sorted.iter().map(|p| p.0).collect();
            let sy: Vec<f64> = sorted.iter().map(|p| p.1).collect();
            let interp = crate::stats::regression::cubic_spline(&sx, &sy, &sx)?;
            let rows: Vec<(String, f64)> = sx
                .iter()
                .zip(interp.iter())
                .map(|(xi, v)| (format!("spline at x = {xi}"), *v))
                .collect();
            Ok(AnalysisResult::Summary(rows))
        }
    }
}

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

/// Number of groups (columns) a table exposes for menu purposes.
pub fn table_group_count(table: &TableInstance) -> usize {
    match table {
        TableInstance::Column(t) => t.columns.len(),
        TableInstance::Grouped(t) => t.group_names.len(),
        TableInstance::Contingency(t) => t.col_labels.len(),
        TableInstance::Xy(t) => t.data.iter().map(|(_, r)| r.len()).max().unwrap_or(1),
    }
}

/// Build the chart for a graph, overlaying a fitted curve when requested.
pub fn build_chart(graph: &Graph, project_tables: &[TableInstance], analyses: &[Analysis]) -> Option<Chart> {
    let table_index = graph
        .table
        .or_else(|| graph.analysis.and_then(|a| analyses.get(a).map(|an| an.table)))?;
    let table = project_tables.get(table_index)?;
    let analysis = graph.analysis.and_then(|a| analyses.get(a));

    let title = if graph.title.is_empty() {
        match &graph.name {
            n if !n.is_empty() => n.clone(),
            _ => table.as_table().name().to_string(),
        }
    } else {
        graph.title.clone()
    };
    let y_label = if graph.y_label.is_empty() {
        table.as_table().name().to_string()
    } else {
        graph.y_label.clone()
    };
    let x_label = graph.x_label.clone();

    let mut chart = match table {
        TableInstance::Xy(t) => {
            if graph.by_replicate {
                // One series per replicate (each point a single measurement).
                xy_chart_by_replicate(
                    title,
                    x_label,
                    y_label,
                    &t.data,
                    graph.style,
                    graph.x_log,
                    graph.y_log,
                )
            } else {
                xy_chart(
                    title,
                    x_label,
                    y_label,
                    &t.data,
                    graph.error,
                    graph.style,
                    graph.x_log,
                    graph.y_log,
                )
            }
        }
        TableInstance::Column(t) => column_chart(
            title,
            y_label,
            &t.columns,
            &t.group_names,
            graph.style,
            graph.error,
            graph.y_log,
        ),
        TableInstance::Grouped(t) => grouped_chart(
            title,
            y_label,
            &t.row_labels,
            &t.group_names,
            &t.data,
            graph.style,
            graph.error,
            graph.y_log,
        ),
        TableInstance::Contingency(t) => {
            // Contingency tables are drawn as grouped bars of the counts.
            let rows: Vec<Vec<Vec<f64>>> = t
                .data
                .iter()
                .map(|r| r.iter().map(|c| vec![*c as f64]).collect())
                .collect();
            grouped_chart(
                title,
                y_label,
                &t.row_labels,
                &t.col_labels,
                &rows,
                SeriesStyle::Bars,
                ErrorKind::None,
                graph.y_log,
            )
        }
    };

    if graph.plot_kind != chart.kind
        && matches!(
            table.flavor(),
            TableFlavor::Column | TableFlavor::Grouped | TableFlavor::Contingency
        )
        && matches!(
            graph.plot_kind,
            PlotKind::Bar | PlotKind::BoxAndWhisker | PlotKind::Column
        )
    {
        chart.kind = graph.plot_kind;
    }

    // Presentation options taken from the graph definition.
    chart.jitter = graph.jitter.clamp(0.0, 0.5);
    chart.legend = graph.legend && chart_wants_legend(&chart);
    chart.width = graph.width;
    chart.height = graph.height;

    if graph.show_fit {
        if let Some(result) = analysis.and_then(|a| a.result.as_ref()) {
            match result {
                AnalysisResult::LinearRegression(r) => {
                    if let TableInstance::Xy(t) = table {
                        if let Some(series) = line_overlay(
                            &xy_x_extent(t),
                            |x| r.intercept + r.slope * x,
                            [220, 40, 40],
                            "Fit",
                        ) {
                            chart.series.push(series);
                        }
                    }
                }
                AnalysisResult::Curve(c) => {
                    if let TableInstance::Xy(t) = table {
                        let model = c.model.clone();
                        let params = c.params.clone();
                        if let Some(series) = line_overlay(
                            &xy_x_extent(t),
                            move |x| {
                                crate::stats::regression::model_by_name(&model)
                                    .map(|m| (m.func)(x, &params))
                                    .unwrap_or(f64::NAN)
                            },
                            [220, 40, 40],
                            "Fit",
                        ) {
                            chart.series.push(series);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Some(chart)
}

/// Whether a legend carries information for this chart.
///
/// Several named series deserve a legend, but the "one series per group with
/// the group name already on the x axis" layout (column bars, boxes, jittered
/// columns) does not.
fn chart_wants_legend(chart: &Chart) -> bool {
    if chart.series.len() < 2 {
        return false;
    }
    let one_point_per_series = chart.series.iter().all(|s| s.points.len() == 1);
    !(one_point_per_series && !chart.categories.is_empty())
}

/// Data-space x extent (min..max) of an XY table, used as the fitting range.
fn xy_x_extent(t: &XyTable) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (x, _) in &t.data {
        lo = lo.min(*x);
        hi = hi.max(*x);
    }
    if lo.is_finite() && hi.is_finite() && hi > lo {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

/// Build a fitted-curve series: sample `f` over `extent` and return it as a
/// line series named `name` (or `None` when there are no finite points).
fn line_overlay(
    extent: &(f64, f64),
    f: impl Fn(f64) -> f64,
    color: [u8; 3],
    name: &str,
) -> Option<Series> {
    let (lo, hi) = *extent;
    let n = 160;
    let mut points = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let x = lo + (hi - lo) * (i as f64 / n as f64);
        let y = f(x);
        if y.is_finite() {
            points.push(crate::plot::DataPoint::new(x, y));
        }
    }
    if points.len() < 2 {
        return None;
    }
    let mut s = Series::new(name, SeriesStyle::Lines, color).with_points(points);
    s.marker = Marker::None;
    s.line_width = 2;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tables::{ColumnTable, ContingencyTable, GroupedTable, XyTable};

    fn column_table() -> TableInstance {
        TableInstance::Column(ColumnTable {
            name: "Doses".into(),
            columns: vec![
                vec![1.0, 1.2, 1.1, 1.3, 1.15],
                vec![2.0, 2.2, 2.1, 2.3, 2.15],
                vec![5.0, 5.2, 5.1, 5.3, 5.15],
            ],
            group_names: vec!["Control".into(), "Low".into(), "High".into()],
        })
    }

    #[test]
    fn descriptive_on_columns() {
        let t = column_table();
        let r = run(&AnalysisSpec::Descriptive, &t).unwrap();
        match r {
            AnalysisResult::Descriptive(list) => {
                assert_eq!(list.len(), 3);
                assert_eq!(list[0].name, "Control");
                assert!((list[0].stats.mean - 1.15).abs() < 1e-9);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn t_test_between_columns() {
        let t = column_table();
        let spec = AnalysisSpec::TTest {
            kind: TTestKind::Unpaired,
            group_a: 0,
            group_b: 1,
        };
        let r = run(&spec, &t).unwrap();
        match r {
            AnalysisResult::TTest(r) => {
                assert!(r.p_value < 0.001);
                assert!((r.mean_diff + 1.0).abs() < 0.05);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn anova_and_posthoc() {
        let t = column_table();
        let r = run(&AnalysisSpec::OneWayAnova, &t).unwrap();
        match &r {
            AnalysisResult::Anova(a) => {
                assert!(a.f > 50.0);
                assert_eq!(a.pairwise.len(), 3);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(!r.summary_rows().is_empty());
        assert!(r.headline().contains("F ="));
    }

    #[test]
    fn curve_fit_on_xy_table() {
        let x: Vec<f64> = (0..12).map(|i| i as f64 * 0.5).collect();
        let data: Vec<(f64, Vec<f64>)> = x
            .iter()
            .map(|&xi| {
                let y = 10.0 * xi / (2.0 + xi);
                (xi, vec![y, y * 1.01, y * 0.99])
            })
            .collect();
        let t = TableInstance::Xy(XyTable {
            name: "Kinetics".into(),
            data,
        });
        let spec = AnalysisSpec::CurveFit {
            model: "Michaelis-Menten".into(),
        };
        let r = run(&spec, &t).unwrap();
        match &r {
            AnalysisResult::Curve(c) => {
                assert!(c.r_squared > 0.99, "{}", c.r_squared);
                assert!((c.params[0] - 10.0).abs() < 0.5);
            }
            other => panic!("unexpected {other:?}"),
        }

        let graph = Graph {
            name: "Kinetics fit".into(),
            table: Some(0),
            analysis: Some(0),
            style: SeriesStyle::LinesAndPoints,
            ..Default::default()
        };
        let analysis = Analysis {
            spec,
            table: 0,
            result: Some(r),
            error: None,
        };
        let chart = build_chart(&graph, &[t], &[analysis]).unwrap();
        // data series + fitted curve
        assert_eq!(chart.series.len(), 2);
        assert_eq!(chart.series[1].name, "Fit");
        assert!(chart.series[1].points.len() > 100);
    }

    #[test]
    fn chi_square_on_contingency() {
        let t = TableInstance::Contingency(ContingencyTable {
            name: "Outcomes".into(),
            row_labels: vec!["Treated".into(), "Control".into()],
            col_labels: vec!["Yes".into(), "No".into()],
            data: vec![vec![80, 20], vec![30, 70]],
        });
        let r = run(&AnalysisSpec::ChiSquare, &t).unwrap();
        match r {
            AnalysisResult::ChiSquare(r) => {
                assert!(r.p_value < 1e-6);
                assert!(r.cramers_v > 0.4);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn two_way_on_grouped() {
        let t = TableInstance::Grouped(GroupedTable {
            name: "Grouped".into(),
            row_labels: vec!["d1".into(), "d2".into()],
            group_names: vec!["A".into(), "B".into()],
            data: vec![
                vec![vec![1.0, 2.0, 3.0], vec![2.0, 3.0, 4.0]],
                vec![vec![5.0, 6.0, 7.0], vec![6.0, 7.0, 8.0]],
            ],
        });
        let r = run(&AnalysisSpec::TwoWayAnova, &t).unwrap();
        match r {
            AnalysisResult::TwoWay(r) => {
                assert!(r.p_a < 0.01);
                assert!(r.ms_error > 0.0);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn legend_is_shown_only_when_informative() {
        // Grouped bars: one series per group, several rows -> legend.
        let grouped = TableInstance::Grouped(GroupedTable {
            name: "Grouped".into(),
            row_labels: vec!["d1".into(), "d2".into()],
            group_names: vec!["A".into(), "B".into()],
            data: vec![
                vec![vec![1.0, 2.0], vec![3.0, 4.0]],
                vec![vec![5.0, 6.0], vec![7.0, 8.0]],
            ],
        });
        let graph = Graph {
            table: Some(0),
            style: SeriesStyle::Bars,
            ..Default::default()
        };
        let chart = build_chart(&graph, &[grouped], &[]).unwrap();
        assert!(chart.legend, "grouped bars should carry a legend");

        // Column bars: one series per group with one point each -> no legend.
        let graph = Graph {
            table: Some(0),
            style: SeriesStyle::Bars,
            ..Default::default()
        };
        let chart = build_chart(&graph, &[column_table()], &[]).unwrap();
        assert!(!chart.legend, "column bars do not need a legend");

        // The option always wins when switched off.
        let graph = Graph {
            table: Some(0),
            style: SeriesStyle::Bars,
            legend: false,
            ..Default::default()
        };
        let chart = build_chart(&graph, &[column_table()], &[]).unwrap();
        assert!(!chart.legend);
    }

    #[test]
    fn graph_presentation_options_reach_the_chart() {
        let graph = Graph {
            table: Some(0),
            style: SeriesStyle::Columns,
            jitter: 0.4,
            width: 640,
            height: 480,
            ..Default::default()
        };
        let chart = build_chart(&graph, &[column_table()], &[]).unwrap();
        assert!((chart.jitter - 0.4).abs() < 1e-9);
        assert_eq!((chart.width, chart.height), (640, 480));
        // Jitter is clamped to a sane range.
        let graph = Graph {
            table: Some(0),
            jitter: 9.0,
            ..Default::default()
        };
        let chart = build_chart(&graph, &[column_table()], &[]).unwrap();
        assert!((chart.jitter - 0.5).abs() < 1e-9);
    }

    #[test]
    fn specs_are_gated_by_table_type() {
        let xy = AnalysisSpec::LinearRegression;
        assert!(xy.applicable_to(TableFlavor::Xy));
        assert!(!xy.applicable_to(TableFlavor::Column));
        assert!(AnalysisSpec::ChiSquare.applicable_to(TableFlavor::Contingency));
        assert!(!AnalysisSpec::OneWayAnova.applicable_to(TableFlavor::Grouped));
        // Running on the wrong table type is an error, not a panic.
        let t = column_table();
        assert!(run(&AnalysisSpec::LinearRegression, &t).is_err());
    }

    #[test]
    fn auc_reports_a_number() {
        let t = TableInstance::Xy(XyTable {
            name: "curve".into(),
            data: vec![(0.0, vec![0.0]), (1.0, vec![1.0]), (2.0, vec![2.0])],
        });
        let r = run(&AnalysisSpec::AreaUnderCurve, &t).unwrap();
        match r {
            AnalysisResult::Summary(rows) => {
                let auc = rows.iter().find(|(k, _)| k.starts_with("AUC (")).unwrap().1;
                assert!((auc - 2.0).abs() < 1e-9);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
