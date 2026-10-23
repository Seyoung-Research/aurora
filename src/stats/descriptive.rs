//! Descriptive statistics for a single sample (a column of numbers).
//!
//! All metrics are computed directly from the sample; confidence intervals use
//! the Student-t distribution.

use serde::{Deserialize, Serialize};

use crate::stats::distributions::t_ppf;

/// Complete set of descriptive statistics for one sample.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DescriptiveStats {
    /// Number of observations in the sample.
    pub n: usize,
    /// Sum of all observations.
    pub sum: f64,
    /// Arithmetic mean (sum / n).
    pub mean: f64,
    /// Sample standard deviation (n - 1 denominator).
    pub std_dev: f64,
    /// Variance (n - 1 denominator).
    pub variance: f64,
    /// Standard error of the mean (std dev / sqrt(n)).
    pub std_error: f64,
    /// Median (50th percentile of the sorted sample).
    pub median: f64,
    pub mode: Option<f64>,
    /// Smallest observed value.
    pub min: f64,
    /// Largest observed value.
    pub max: f64,
    /// Range (max - min).
    pub range: f64,
    /// First quartile (25th percentile) via linear interpolation.
    pub q1: f64,
    /// Third quartile (75th percentile) via linear interpolation.
    pub q3: f64,
    /// Interquartile range (q3 - q1).
    pub iqr: f64,
    /// Geometric mean (only when all values are > 0).
    pub geometric_mean: Option<f64>,
    /// Coefficient of variation (%) = SD / mean * 100.
    pub cv_percent: Option<f64>,
    /// Lower bound of the 95% confidence interval of the mean.
    pub ci95_lower: f64,
    /// Upper bound of the 95% confidence interval of the mean.
    pub ci95_upper: f64,
}

impl DescriptiveStats {
    /// Half-width of the 95% confidence interval of the mean.
    pub fn ci95_half_width(&self) -> f64 {
        (self.ci95_upper - self.ci95_lower) / 2.0
    }
}

/// Linear-interpolation quantile (the "type 7" / Excel `PERCENTILE.INC`
/// convention) of a sample that must be sorted ascending.
///
/// Returns `f64::NAN` for an empty sample and the single value for a
/// one-element sample.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    // Linear interpolation between order statistics (type 7 / Excel PERCENTILE).
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return sorted[0];
    }
    let p = p.clamp(0.0, 1.0);
    let h = (n as f64 - 1.0) * p;
    let lo = h.floor() as usize;
    let hi = h.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = h - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

/// Compute descriptive statistics for a sample.
///
/// Returns `None` for an empty slice. A single observation yields degenerate
/// statistics (zero variance, confidence interval equal to the value); a sample
/// of two or more uses the Student-t based confidence interval of the mean.
pub fn calculate_descriptive(data: &[f64]) -> Option<DescriptiveStats> {
    if data.is_empty() {
        return None;
    }

    let n = data.len();
    let nf = n as f64;
    let sum: f64 = data.iter().sum();
    let mean = sum / nf;

    // Sample variance (n - 1). For n == 1 the variance is undefined; report 0.
    let variance = if n > 1 {
        data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (nf - 1.0)
    } else {
        0.0
    };
    let std_dev = variance.sqrt();
    let std_error = if n > 0 { std_dev / nf.sqrt() } else { 0.0 };

    let mut sorted: Vec<f64> = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median = quantile(&sorted, 0.5);
    let q1 = quantile(&sorted, 0.25);
    let q3 = quantile(&sorted, 0.75);
    let min = sorted[0];
    let max = sorted[n - 1];

    // Mode: the most frequent value (rounded to 6 dp to make floats comparable).
    let mode = {
        use std::collections::HashMap;
        let mut counts: HashMap<i64, (usize, f64)> = HashMap::new();
        for &v in data {
            let key = (v * 1e6).round() as i64;
            let entry = counts.entry(key).or_insert((0, v));
            entry.0 += 1;
        }
        let mut best: Option<(usize, f64)> = None;
        for (_, (c, v)) in counts.iter() {
            if let Some((bc, _)) = best {
                if *c > bc {
                    best = Some((*c, *v));
                }
            } else {
                best = Some((*c, *v));
            }
        }
        match best {
            Some((c, v)) if c > 1 => Some(v),
            _ => None,
        }
    };

    let geometric_mean = if data.iter().all(|&x| x > 0.0) {
        let ln_sum: f64 = data.iter().map(|x| x.ln()).sum();
        Some((ln_sum / nf).exp())
    } else {
        None
    };

    let cv_percent = if mean != 0.0 {
        Some(std_dev / mean.abs() * 100.0)
    } else {
        None
    };

    // 95% CI of the mean using the t distribution.
    let (ci95_lower, ci95_upper) = if n > 1 {
        let tcrit = t_ppf(0.975, (nf - 1.0).max(1.0));
        let hw = tcrit * std_error;
        (mean - hw, mean + hw)
    } else {
        (mean, mean)
    };

    Some(DescriptiveStats {
        n,
        sum,
        mean,
        std_dev,
        variance,
        std_error,
        median,
        mode,
        min,
        max,
        range: max - min,
        q1,
        q3,
        iqr: q3 - q1,
        geometric_mean,
        cv_percent,
        ci95_lower,
        ci95_upper,
    })
}

/// Descriptive statistics for each column of a table (rows = observations).
///
/// The returned vector has one entry per column of `columns`, in the same order;
/// entries for empty columns are `None`.
pub fn descriptive_by_columns(columns: &[Vec<f64>]) -> Vec<Option<DescriptiveStats>> {
    columns.iter().map(|c| calculate_descriptive(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_stats() {
        let d = [1.0, 2.0, 3.0, 4.0, 5.0];
        let s = calculate_descriptive(&d).unwrap();
        assert_eq!(s.n, 5);
        assert!((s.mean - 3.0).abs() < 1e-12);
        assert!((s.median - 3.0).abs() < 1e-12);
        assert!((s.min - 1.0).abs() < 1e-12);
        assert!((s.max - 5.0).abs() < 1e-12);
        assert!((s.range - 4.0).abs() < 1e-12);
        assert!((s.variance - 2.5).abs() < 1e-12);
        assert!((s.std_dev - 2.5_f64.sqrt()).abs() < 1e-12);
        assert!((s.q1 - 2.0).abs() < 1e-12);
        assert!((s.q3 - 4.0).abs() < 1e-12);
    }

    #[test]
    fn empty_is_none() {
        assert!(calculate_descriptive(&[]).is_none());
    }

    #[test]
    fn single_value() {
        let s = calculate_descriptive(&[7.0]).unwrap();
        assert_eq!(s.n, 1);
        assert!((s.mean - 7.0).abs() < 1e-12);
        assert!((s.std_dev).abs() < 1e-12);
        assert!((s.ci95_lower - 7.0).abs() < 1e-12);
    }

    #[test]
    fn ci_contains_mean() {
        let d = [1.2, 1.3, 1.1, 1.25, 1.18];
        let s = calculate_descriptive(&d).unwrap();
        assert!(s.ci95_lower < s.mean && s.mean < s.ci95_upper);
    }
}
