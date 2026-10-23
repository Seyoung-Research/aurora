//! Statistical routines behind Aurora's analyses: descriptive statistics,
//! hypothesis tests and regression / curve fitting.
//!
//! The module is split into four submodules, all of them public so that the
//! desktop app, the command-line self-test and external users of the engine can
//! reach them:
//!
//! - [`descriptive`] — summary statistics for one sample or for every column
//!   of a table.
//! - [`comparison`] — t-tests, ANOVA and non-parametric comparisons, each
//!   returning the raw statistic plus a p-value and (where meaningful) confidence
//!   intervals and effect sizes.
//! - [`regression`] — linear regression, Deming (errors-in-variables) regression,
//!   non-linear least squares via Levenberg-Marquardt, and curve utilities
//!   (trapezoidal area under the curve, LOWESS smoothing, cubic spline).
//! - [`distributions`] — the statistical distributions (normal, Student-t, F,
//!   chi-square) used to turn statistics into p-values and to invert them.

pub mod descriptive;
pub mod comparison;
pub mod regression;
pub mod distributions;
