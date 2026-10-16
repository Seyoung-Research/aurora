//! Regression and curve fitting.
//!
//! - Simple linear regression (OLS) with standard errors and confidence bands.
//! - Deming (errors-in-variables) regression.
//! - Non-linear least squares via Levenberg-Marquardt with a model library
//!   (sigmoidal dose-response, exponential growth/decay, Michaelis-Menten,
//!   Gompertz, log, power).
//! - Curve utilities: trapezoidal AUC, LOWESS smoothing, natural cubic spline.

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::stats::distributions::t_ppf;

// ---------------------------------------------------------------------------
// Linear regression
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LinearRegressionResult {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    /// Standard error of the slope.
    pub slope_se: f64,
    /// Standard error of the intercept.
    pub intercept_se: f64,
    /// p-value for the slope being non-zero.
    pub slope_p: f64,
    pub n: usize,
    /// 95% confidence interval of the slope.
    pub slope_ci95: (f64, f64),
}

/// Ordinary least squares fit of `y = intercept + slope * x`.
pub fn simple_linear_regression(x: &[f64], y: &[f64]) -> Result<LinearRegressionResult> {
    if x.len() != y.len() {
        bail!("X and Y must have the same length");
    }
    let n = x.len();
    if n < 3 {
        bail!("At least 3 points are required for linear regression");
    }
    let nf = n as f64;
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y).map(|(a, b)| a * b).sum();
    let sum_xx: f64 = x.iter().map(|a| a * a).sum();
    let mean_x = sum_x / nf;
    let mean_y = sum_y / nf;

    let sxx = sum_xx - sum_x * sum_x / nf;
    let sxy = sum_xy - sum_x * sum_y / nf;
    if sxx.abs() < f64::EPSILON {
        bail!("All X values are identical");
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;

    let ss_tot: f64 = y.iter().map(|v| (v - mean_y).powi(2)).sum();
    let ss_res: f64 = x
        .iter()
        .zip(y)
        .map(|(xi, yi)| (yi - (intercept + slope * xi)).powi(2))
        .sum();
    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    let df = nf - 2.0;
    let mse = ss_res / df;
    let slope_se = (mse / sxx).sqrt();
    let intercept_se = (mse * (1.0 / nf + mean_x * mean_x / sxx)).sqrt();
    let t_stat = slope / slope_se;
    let slope_p = crate::stats::distributions::t_two_sided_p(t_stat, df);
    let tcrit = t_ppf(0.975, df);

    Ok(LinearRegressionResult {
        slope,
        intercept,
        r_squared,
        slope_se,
        intercept_se,
        slope_p,
        n,
        slope_ci95: (slope - tcrit * slope_se, slope + tcrit * slope_se),
    })
}

/// Deming regression (total least squares / errors-in-variables).
///
/// `lambda` is the ratio of the variances of the errors in x and y
/// (`lambda = 1` for equal errors).
pub fn deming_regression(
    x: &[f64],
    y: &[f64],
    lambda: f64,
) -> Result<LinearRegressionResult> {
    if x.len() != y.len() {
        bail!("X and Y must have the same length");
    }
    let n = x.len();
    if n < 3 {
        bail!("At least 3 points are required");
    }
    let nf = n as f64;
    let mean_x = x.iter().sum::<f64>() / nf;
    let mean_y = y.iter().sum::<f64>() / nf;

    let mut sxx = 0.0;
    let mut syy = 0.0;
    let mut sxy = 0.0;
    for (xi, yi) in x.iter().zip(y) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let sxx = sxx / (nf - 1.0);
    let syy = syy / (nf - 1.0);
    let sxy = sxy / (nf - 1.0);

    let slope = if sxy.abs() < f64::EPSILON {
        0.0
    } else {
        (syy - lambda * sxx + ((syy - lambda * sxx).powi(2) + 4.0 * lambda * sxy * sxy).sqrt())
            / (2.0 * sxy)
    };
    let intercept = mean_y - slope * mean_x;

    let ss_res: f64 = x
        .iter()
        .zip(y)
        .map(|(xi, yi)| (yi - (intercept + slope * xi)).powi(2))
        .sum();
    let ss_tot: f64 = y.iter().map(|v| (v - mean_y).powi(2)).sum();
    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    Ok(LinearRegressionResult {
        slope,
        intercept,
        r_squared,
        slope_se: f64::NAN,
        intercept_se: f64::NAN,
        slope_p: f64::NAN,
        n,
        slope_ci95: (f64::NAN, f64::NAN),
    })
}

// ---------------------------------------------------------------------------
// Non-linear curve fitting (Levenberg-Marquardt)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CurveFitResult {
    pub model: String,
    pub param_names: Vec<String>,
    pub params: Vec<f64>,
    /// Approximate standard errors of the parameters (from J^T J).
    pub param_std_errors: Vec<f64>,
    pub r_squared: f64,
    pub rmse: f64,
    pub iterations: usize,
    pub converged: bool,
}

impl CurveFitResult {
    /// Evaluate the fitted curve at `x`.
    pub fn eval(&self, x: f64) -> Option<f64> {
        model_by_name(&self.model).map(|_| evaluate(&self.model, x, &self.params))
    }
}

/// A non-linear model: a name, its parameter names, a sensible initial guess
/// and the function itself.
#[derive(Clone, Copy)]
pub struct ModelDef {
    pub name: &'static str,
    pub param_names: &'static [&'static str],
    pub func: fn(f64, &[f64]) -> f64,
    pub init: fn(&[f64], &[f64]) -> Vec<f64>,
}

fn evaluate(model: &str, x: f64, p: &[f64]) -> f64 {
    match model_by_name(model) {
        Some(m) => (m.func)(x, p),
        None => f64::NAN,
    }
}

/// The library of built-in models.
pub fn model_library() -> Vec<ModelDef> {
    vec![
        ModelDef {
            name: "Sigmoidal dose-response (4PL)",
            param_names: &["Bottom", "Top", "logEC50", "HillSlope"],
            func: |x, p| {
                let bottom = p[0];
                let top = p[1];
                let logec50 = p[2];
                let hill = p[3];
                bottom + (top - bottom) / (1.0 + 10.0_f64.powf((logec50 - x) * hill))
            },
            init: |_x, y| {
                let ymin = y.iter().cloned().fold(f64::INFINITY, f64::min);
                let ymax = y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                vec![ymin, ymax, 0.0, 1.0]
            },
        },
        ModelDef {
            name: "Exponential growth",
            param_names: &["Y0", "k"],
            func: |x, p| p[0] * (p[1] * x).exp(),
            init: |_x, y| {
                let y0 = y.first().copied().unwrap_or(1.0).max(1e-6);
                vec![y0, 0.1]
            },
        },
        ModelDef {
            name: "Exponential decay (2 param)",
            param_names: &["Y0", "k"],
            func: |x, p| p[0] * (-p[1] * x).exp(),
            init: |_x, y| {
                let y0 = y.first().copied().unwrap_or(1.0).max(1e-6);
                vec![y0, 0.1]
            },
        },
        ModelDef {
            name: "Exponential decay (3 param, plateau)",
            param_names: &["Plateau", "Span", "k"],
            func: |x, p| p[0] + p[1] * (-p[2] * x).exp(),
            init: |_x, y| {
                let ymin = y.iter().cloned().fold(f64::INFINITY, f64::min);
                let ymax = y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                vec![ymin, ymax - ymin, 0.1]
            },
        },
        ModelDef {
            name: "Michaelis-Menten",
            param_names: &["Vmax", "Km"],
            func: |x, p| {
                let denom = p[1] + x;
                if denom.abs() < 1e-12 {
                    0.0
                } else {
                    p[0] * x / denom
                }
            },
            init: |x, y| {
                let ymax = y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let xmean = x.iter().sum::<f64>() / x.len().max(1) as f64;
                vec![ymax, xmean.abs().max(1e-6)]
            },
        },
        ModelDef {
            name: "Gompertz",
            param_names: &["Plateau", "b", "c"],
            func: |x, p| p[0] * (-(-p[1] * (x - p[2])).exp()).exp(),
            init: |x, y| {
                let ymax = y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let xmean = x.iter().sum::<f64>() / x.len().max(1) as f64;
                vec![ymax, 0.1, xmean]
            },
        },
        ModelDef {
            name: "Logarithmic",
            param_names: &["Intercept", "Slope"],
            func: |x, p| p[0] + p[1] * x.max(1e-12).ln(),
            init: |_x, y| {
                let ymean = y.iter().sum::<f64>() / y.len().max(1) as f64;
                vec![ymean, 1.0]
            },
        },
        ModelDef {
            name: "Power",
            param_names: &["A", "B"],
            func: |x, p| p[0] * x.max(0.0).powf(p[1]),
            init: |_x, y| {
                let y0 = y.iter().cloned().filter(|v| *v > 0.0).fold(f64::INFINITY, f64::min);
                vec![if y0.is_finite() { y0 } else { 1.0 }, 1.0]
            },
        },
    ]
}

/// Look up a model definition by name.
pub fn model_by_name(name: &str) -> Option<ModelDef> {
    model_library().into_iter().find(|m| m.name == name)
}

/// Fit `x`/`y` to the named model using Levenberg-Marquardt least squares.
pub fn fit_curve(model_name: &str, x: &[f64], y: &[f64]) -> Result<CurveFitResult> {
    if x.len() != y.len() {
        bail!("X and Y must have the same length");
    }
    let n = x.len();
    if n < 3 {
        bail!("At least 3 points are required to fit a curve");
    }
    let model = model_by_name(model_name)
        .ok_or_else(|| anyhow!("Unknown model '{model_name}'"))?;

    let params0 = (model.init)(x, y);
    let p = model.param_names.len();
    let (params, converged, iterations, sse) = levenberg_marquardt(
        x,
        y,
        &params0,
        |xi, params| (model.func)(xi, params),
    )?;

    let mean_y = y.iter().sum::<f64>() / n as f64;
    let ss_tot: f64 = y.iter().map(|v| (v - mean_y).powi(2)).sum();
    let r_squared = if ss_tot > 0.0 {
        1.0 - sse / ss_tot
    } else {
        0.0
    };
    let rmse = (sse / n as f64).sqrt();

    // Parameter standard errors from the Jacobian at the solution.
    let j = jacobian(x, &params, |xi, params| (model.func)(xi, params));
    let jtj = jtj(&j);
    let std_errors = invert(&jtj)
        .map(|inv| {
            let mse = sse / (n as f64 - p as f64);
            inv.diagonal().iter().map(|&v| (v * mse).max(0.0).sqrt()).collect()
        })
        .unwrap_or_else(|_| vec![f64::NAN; p]);

    Ok(CurveFitResult {
        model: model_name.to_string(),
        param_names: model.param_names.iter().map(|s| s.to_string()).collect(),
        params,
        param_std_errors: std_errors,
        r_squared,
        rmse,
        iterations,
        converged,
    })
}

// ---- Levenberg-Marquardt core ----

/// Residuals `r_i = y_i - f(x_i, params)`.
fn residuals(x: &[f64], y: &[f64], params: &[f64], f: impl Fn(f64, &[f64]) -> f64) -> Vec<f64> {
    x.iter()
        .zip(y)
        .map(|(xi, yi)| yi - f(*xi, params))
        .collect()
}

/// Numeric Jacobian `J[k][q] = ∂f(x_k, p)/∂p_q` via central differences.
///
/// Note: with residuals defined as `r = y - f`, the Gauss-Newton step for
/// minimising `Σ r²` is `δ = (JᵀJ)⁻¹ Jᵀ r` using this (positive) Jacobian.
fn jacobian(
    x: &[f64],
    params: &[f64],
    f: impl Fn(f64, &[f64]) -> f64,
) -> Vec<Vec<f64>> {
    let n = x.len();
    let p = params.len();
    let mut j = vec![vec![0.0; p]; n];
    for (k, jk) in j.iter_mut().enumerate().take(n) {
        let xk = x[k];
        for (q, jkq) in jk.iter_mut().enumerate().take(p) {
            let h = (params[q].abs() * 1e-6).max(1e-8);
            let mut pp = params.to_vec();
            let mut pm = params.to_vec();
            pp[q] += h;
            pm[q] -= h;
            *jkq = (f(xk, &pp) - f(xk, &pm)) / (2.0 * h);
        }
    }
    j
}

/// Normal-equation matrix `J^T J`.
fn jtj(j: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let p = j[0].len();
    let mut a = vec![vec![0.0; p]; p];
    for row in j {
        for i in 0..p {
            for k in 0..p {
                a[i][k] += row[i] * row[k];
            }
        }
    }
    a
}

/// `J^T r`.
fn jtr(j: &[Vec<f64>], r: &[f64]) -> Vec<f64> {
    let p = j[0].len();
    let mut v = vec![0.0; p];
    for (row, ri) in j.iter().zip(r) {
        for i in 0..p {
            v[i] += row[i] * ri;
        }
    }
    v
}

fn sse(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum()
}

/// Levenberg-Marquardt with adaptive damping.
///
/// Returns `(params, converged, iterations, sse)`.
fn levenberg_marquardt(
    x: &[f64],
    y: &[f64],
    params0: &[f64],
    f: impl Fn(f64, &[f64]) -> f64,
) -> Result<(Vec<f64>, bool, usize, f64)> {
    let p = params0.len();
    let mut params = params0.to_vec();
    let mut r = residuals(x, y, &params, &f);
    let mut cur_sse = sse(&r);
    let mut lambda = 1e-3;
    let max_iter = 200;
    // LM is done when many consecutive steps fail to improve: the damping has
    // grown so large that steps are negligible and we are at a minimum.
    let max_rejections = 12;
    let mut rejections = 0;
    let mut converged = false;
    let mut used_iters = max_iter;

    for iter in 0..max_iter {
        let j = jacobian(x, &params, &f);
        let a = jtj(&j);
        let g = jtr(&j, &r);

        // Solve (A + lambda * diag(A)) delta = g
        let mut augmented = a.clone();
        for i in 0..p {
            augmented[i][i] += lambda * a[i][i].max(1e-12);
        }
        let delta = match solve(&augmented, &g) {
            Ok(d) => d,
            Err(_) => {
                let _ = &g;
                lambda *= 10.0;
                rejections += 1;
                if rejections >= max_rejections {
                    used_iters = iter + 1;
                    break;
                }
                continue;
            }
        };

        let mut candidate = params.clone();
        for i in 0..p {
            candidate[i] += delta[i];
        }
        let new_r = residuals(x, y, &candidate, &f);
        let new_sse = sse(&new_r);

        if new_sse.is_finite() && new_sse < cur_sse {
            let improvement = (cur_sse - new_sse) / cur_sse.max(1e-300);
            params = candidate;
            r = new_r;
            cur_sse = new_sse;
            lambda = (lambda * 0.3).max(1e-12);
            rejections = 0;
            used_iters = iter + 1;
            if improvement < 1e-14 {
                converged = true;
                break;
            }
        } else {
            lambda *= 10.0;
            rejections += 1;
            if rejections >= max_rejections {
                used_iters = iter + 1;
                break;
            }
        }
    }

    // If the final sweep of steps all failed, the fit is at a (local) minimum.
    if rejections >= max_rejections {
        converged = true;
    }
    Ok((params, converged, used_iters, cur_sse))
}

// ---- Small dense linear algebra (Gaussian elimination) ----

fn solve(a: &[Vec<f64>], b: &[f64]) -> Result<Vec<f64>> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut rhs = b.to_vec();

    for col in 0..n {
        // partial pivoting
        let mut piv = col;
        for row in (col + 1)..n {
            if m[row][col].abs() > m[piv][col].abs() {
                piv = row;
            }
        }
        if m[piv][col].abs() < 1e-300 {
            bail!("Singular matrix");
        }
        m.swap(col, piv);
        rhs.swap(col, piv);

        let diag = m[col][col];
        for row in (col + 1)..n {
            let factor = m[row][col] / diag;
            for k in col..n {
                m[row][k] -= factor * m[col][k];
            }
            rhs[row] -= factor * rhs[col];
        }
    }

    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut s = rhs[row];
        for k in (row + 1)..n {
            s -= m[row][k] * x[k];
        }
        x[row] = s / m[row][row];
    }
    Ok(x)
}

/// Simple wrapper so we can call `.diagonal()` on an inverted matrix result.
struct Mat {
    data: Vec<Vec<f64>>,
}
impl Mat {
    fn diagonal(&self) -> Vec<f64> {
        (0..self.data.len()).map(|i| self.data[i][i]).collect()
    }
}

fn invert(a: &[Vec<f64>]) -> Result<Mat> {
    let n = a.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut inv = vec![vec![0.0; n]; n];
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }

    for col in 0..n {
        let mut piv = col;
        for row in (col + 1)..n {
            if m[row][col].abs() > m[piv][col].abs() {
                piv = row;
            }
        }
        if m[piv][col].abs() < 1e-300 {
            bail!("Singular matrix");
        }
        m.swap(col, piv);
        inv.swap(col, piv);

        let diag = m[col][col];
        for k in 0..n {
            m[col][k] /= diag;
            inv[col][k] /= diag;
        }
        for row in 0..n {
            if row != col {
                let factor = m[row][col];
                if factor != 0.0 {
                    for k in 0..n {
                        m[row][k] -= factor * m[col][k];
                        inv[row][k] -= factor * inv[col][k];
                    }
                }
            }
        }
    }
    Ok(Mat { data: inv })
}

// ---------------------------------------------------------------------------
// Curve utilities
// ---------------------------------------------------------------------------

/// Area under the curve by the trapezoidal rule.
pub fn trapezoidal_auc(x: &[f64], y: &[f64]) -> Result<f64> {
    if x.len() != y.len() || x.len() < 2 {
        bail!("Need at least 2 paired points");
    }
    let mut auc = 0.0;
    for i in 0..x.len() - 1 {
        auc += (x[i + 1] - x[i]) * (y[i] + y[i + 1]) / 2.0;
    }
    Ok(auc)
}

/// LOWESS smoothing. Returns the smoothed y values (same length as input).
pub fn lowess(x: &[f64], y: &[f64], frac: f64, iters: usize) -> Result<Vec<f64>> {
    let n = x.len();
    if n != y.len() || n < 3 {
        bail!("Need at least 3 paired points");
    }
    let frac = frac.clamp(0.05, 1.0);
    let k = ((frac * n as f64).round() as usize).max(2).min(n);
    let mut smoothed = vec![0.0; n];
    let mut residuals = vec![0.0; n];
    let mut robustness = vec![1.0; n];

    for iter in 0..=iters {
        for i in 0..n {
            // find the k nearest x to xi
            let mut dists: Vec<(f64, usize)> = (0..n)
                .map(|j| ((x[j] - x[i]).abs(), j))
                .collect();
            dists.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let max_d = dists[k - 1].0.max(1e-12);

            // tricube weights
            let mut wsum = 0.0;
            let mut wx = 0.0;
            let mut wy = 0.0;
            let mut wxx = 0.0;
            let mut wxy = 0.0;
            for &(d, j) in &dists[..k] {
                let u = (d / max_d).min(1.0);
                let w = (1.0 - u.powi(3)).powi(3) * robustness[j];
                wsum += w;
                wx += w * x[j];
                wy += w * y[j];
                wxx += w * x[j] * x[j];
                wxy += w * x[j] * y[j];
            }
            let denom = wsum * wxx - wx * wx;
            if denom.abs() < 1e-12 {
                smoothed[i] = if wsum > 0.0 { wy / wsum } else { y[i] };
            } else {
                let b = (wsum * wxy - wx * wy) / denom;
                let a = (wy - b * wx) / wsum;
                smoothed[i] = a + b * x[i];
            }
        }
        for i in 0..n {
            residuals[i] = y[i] - smoothed[i];
        }
        if iter < iters {
            let abs_res: Vec<f64> = residuals.iter().map(|r| r.abs()).collect();
            let mut sorted = abs_res.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = sorted[sorted.len() / 2].max(1e-12);
            for i in 0..n {
                let u = (abs_res[i] / (6.0 * median)).min(1.0);
                robustness[i] = (1.0 - u * u).powi(2);
            }
        }
    }
    Ok(smoothed)
}

/// Natural cubic spline through the given points, evaluated on `xq`.
///
/// `x` must be sorted ascending.
pub fn cubic_spline(x: &[f64], y: &[f64], xq: &[f64]) -> Result<Vec<f64>> {
    let n = x.len();
    if n != y.len() || n < 3 {
        bail!("Need at least 3 paired points");
    }
    // Solve for second derivatives (natural boundary conditions).
    let mut h = vec![0.0; n - 1];
    for i in 0..n - 1 {
        h[i] = x[i + 1] - x[i];
    }
    let mut alpha = vec![0.0; n];
    for i in 1..n - 1 {
        alpha[i] = 3.0 * (y[i + 1] - y[i]) / h[i] - 3.0 * (y[i] - y[i - 1]) / h[i - 1];
    }
    let mut l = vec![1.0; n];
    let mut mu = vec![0.0; n];
    let mut z = vec![0.0; n];
    for i in 1..n - 1 {
        l[i] = 2.0 * (x[i + 1] - x[i - 1]) - h[i - 1] * mu[i - 1];
        mu[i] = h[i] / l[i];
        z[i] = (alpha[i] - h[i - 1] * z[i - 1]) / l[i];
    }
    let mut c = vec![0.0; n];
    let mut b = vec![0.0; n];
    let mut d = vec![0.0; n];
    for j in (0..n - 1).rev() {
        c[j] = z[j] - mu[j] * c[j + 1];
        b[j] = (y[j + 1] - y[j]) / h[j] - h[j] * (c[j + 1] + 2.0 * c[j]) / 3.0;
        d[j] = (c[j + 1] - c[j]) / (3.0 * h[j]);
    }

    let mut out = Vec::with_capacity(xq.len());
    for &q in xq {
        // locate interval
        let mut j = 0;
        if q <= x[0] {
            j = 0;
        } else if q >= x[n - 1] {
            j = n - 2;
        } else {
            for i in 0..n - 1 {
                if q >= x[i] && q <= x[i + 1] {
                    j = i;
                    break;
                }
            }
        }
        let dx = q - x[j];
        out.push(y[j] + b[j] * dx + c[j] * dx * dx + d[j] * dx * dx * dx);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_recovers_line() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [2.1, 3.9, 6.1, 8.0, 10.2];
        let r = simple_linear_regression(&x, &y).unwrap();
        assert!((r.slope - 2.0).abs() < 0.05, "slope {}", r.slope);
        assert!((r.r_squared - 0.999).abs() < 0.01);
        assert!(r.slope_p < 0.001);
    }

    #[test]
    fn deming_runs() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [2.0, 4.0, 6.0, 8.0, 10.0];
        let r = deming_regression(&x, &y, 1.0).unwrap();
        assert!((r.slope - 2.0).abs() < 0.1, "slope {}", r.slope);
    }

    #[test]
    fn fit_sigmoid() {
        // Simulate a 4PL with known parameters and fit it back.
        let true_p = [0.0, 100.0, 1.0, 1.0];
        let x: Vec<f64> = (-6..=10).map(|i| i as f64 * 0.5).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|&xi| true_p[0] + (true_p[1] - true_p[0]) / (1.0 + 10.0_f64.powf((true_p[2] - xi) * true_p[3])))
            .collect();
        let fit = fit_curve("Sigmoidal dose-response (4PL)", &x, &y).unwrap();
        assert!(fit.r_squared > 0.99, "r2 {}", fit.r_squared);
        assert!((fit.params[2] - 1.0).abs() < 0.05, "logEC50 {}", fit.params[2]);
        assert!((fit.params[3] - 1.0).abs() < 0.05, "hill {}", fit.params[3]);
        assert!(fit.converged);
        assert!(fit.param_std_errors.iter().all(|e| !e.is_nan()));
    }

    #[test]
    fn fit_michaelis_menten() {
        let true_vmax = 10.0;
        let true_km = 2.0;
        let x: Vec<f64> = (1..=8).map(|i| i as f64 * 0.5).collect();
        let y: Vec<f64> = x.iter().map(|&xi| true_vmax * xi / (true_km + xi)).collect();
        let fit = fit_curve("Michaelis-Menten", &x, &y).unwrap();
        assert!(fit.r_squared > 0.99, "r2 {}", fit.r_squared);
    }

    #[test]
    fn auc_trapezoid() {
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 1.0, 2.0];
        // trapezoids: (1)(0.5) + (1)(1.5) = 2.0
        let auc = trapezoidal_auc(&x, &y).unwrap();
        assert!((auc - 2.0).abs() < 1e-12);
    }

    #[test]
    fn lowess_smooths() {
        let x: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|&v| 2.0 * v + 1.0).collect();
        let s = lowess(&x, &y, 0.5, 0).unwrap();
        // A straight line should be reproduced almost exactly.
        for (a, b) in s.iter().zip(&y) {
            assert!((a - b).abs() < 0.5, "got {a} want {b}");
        }
    }

    #[test]
    fn spline_interpolates() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 1.0, 4.0, 9.0];
        let xq = [0.0, 1.0, 2.0, 3.0];
        let s = cubic_spline(&x, &y, &xq).unwrap();
        for i in 0..4 {
            assert!((s[i] - y[i]).abs() < 1e-9, "at {i}: {} vs {}", s[i], y[i]);
        }
    }
}
