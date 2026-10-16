//! Hypothesis tests: t-tests, ANOVA, and non-parametric comparisons.
//!
//! Every test returns both the raw statistic and a p-value, plus (where it
//! makes sense) confidence intervals and effect sizes.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::stats::descriptive::calculate_descriptive;
use crate::stats::distributions::{
    chisq_upper_p, f_upper_p, normal_sf, t_ppf, t_two_sided_p,
};

// ---------------------------------------------------------------------------
// t-tests
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum TTestKind {
    Unpaired,
    Welch,
    Paired,
    OneSample,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TTestResult {
    pub kind: TTestKind,
    pub t: f64,
    pub df: f64,
    pub p_value: f64,
    pub mean_a: f64,
    pub mean_b: f64,
    pub mean_diff: f64,
    /// 95% confidence interval of the difference in means.
    pub ci95_diff: (f64, f64),
    /// Cohen's d effect size (unpaired) or dz (paired/one-sample).
    pub effect_size: f64,
    pub n_a: usize,
    pub n_b: usize,
}

impl TTestResult {
    pub fn significant(&self) -> bool {
        self.p_value < 0.05
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn variance(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)
}

/// Unpaired t-test. `welch = true` uses Welch's correction (unequal variances).
pub fn t_test_unpaired(a: &[f64], b: &[f64], welch: bool) -> Result<TTestResult> {
    if a.len() < 2 || b.len() < 2 {
        return Err(anyhow!("Each group needs at least 2 observations"));
    }
    let n1 = a.len() as f64;
    let n2 = b.len() as f64;
    let m1 = mean(a);
    let m2 = mean(b);
    let v1 = variance(a);
    let v2 = variance(b);
    let diff = m1 - m2;

    let (t, df, se) = if welch {
        let se = (v1 / n1 + v2 / n2).sqrt();
        let t = diff / se;
        let df = (v1 / n1 + v2 / n2).powi(2)
            / ((v1 / n1).powi(2) / (n1 - 1.0) + (v2 / n2).powi(2) / (n2 - 1.0));
        (t, df, se)
    } else {
        let pooled = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
        let se = (pooled * (1.0 / n1 + 1.0 / n2)).sqrt();
        let t = diff / se;
        (t, n1 + n2 - 2.0, se)
    };

    let p = t_two_sided_p(t, df);
    let tcrit = t_ppf(0.975, df);
    let pooled_sd = (((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0)).sqrt();
    let d = if pooled_sd > 0.0 { diff / pooled_sd } else { 0.0 };

    Ok(TTestResult {
        kind: if welch { TTestKind::Welch } else { TTestKind::Unpaired },
        t,
        df,
        p_value: p,
        mean_a: m1,
        mean_b: m2,
        mean_diff: diff,
        ci95_diff: (diff - tcrit * se, diff + tcrit * se),
        effect_size: d,
        n_a: a.len(),
        n_b: b.len(),
    })
}

/// Paired t-test (one sample on the differences).
pub fn t_test_paired(a: &[f64], b: &[f64]) -> Result<TTestResult> {
    if a.len() != b.len() {
        return Err(anyhow!("Paired samples must have the same length"));
    }
    if a.len() < 2 {
        return Err(anyhow!("Need at least 2 paired observations"));
    }
    let diffs: Vec<f64> = a.iter().zip(b.iter()).map(|(x, y)| x - y).collect();
    let mut r = t_test_one_sample(&diffs, 0.0)?;
    r.kind = TTestKind::Paired;
    r.mean_a = mean(a);
    r.mean_b = mean(b);
    Ok(r)
}

/// One-sample t-test against a hypothetical mean `mu`.
pub fn t_test_one_sample(data: &[f64], mu: f64) -> Result<TTestResult> {
    if data.len() < 2 {
        return Err(anyhow!("Need at least 2 observations"));
    }
    let n = data.len() as f64;
    let m = mean(data);
    let sd = variance(data).sqrt();
    let se = sd / n.sqrt();
    let t = (m - mu) / se;
    let df = n - 1.0;
    let p = t_two_sided_p(t, df);
    let tcrit = t_ppf(0.975, df);
    let dz = if sd > 0.0 { (m - mu) / sd } else { 0.0 };

    Ok(TTestResult {
        kind: TTestKind::OneSample,
        t,
        df,
        p_value: p,
        mean_a: m,
        mean_b: mu,
        mean_diff: m - mu,
        ci95_diff: (m - mu - tcrit * se, m - mu + tcrit * se),
        effect_size: dz,
        n_a: data.len(),
        n_b: 0,
    })
}

// ---------------------------------------------------------------------------
// One-way ANOVA
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PairwiseComparison {
    pub group_a: usize,
    pub group_b: usize,
    pub mean_diff: f64,
    /// Bonferroni-adjusted p-value.
    pub p_adjusted: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnovaResult {
    pub ss_between: f64,
    pub ss_within: f64,
    pub ss_total: f64,
    pub df_between: f64,
    pub df_within: f64,
    pub ms_between: f64,
    pub ms_within: f64,
    pub f: f64,
    pub p_value: f64,
    /// Eta-squared effect size.
    pub eta_squared: f64,
    pub group_means: Vec<f64>,
    pub group_ns: Vec<usize>,
    /// Bonferroni-adjusted pairwise comparisons.
    pub pairwise: Vec<PairwiseComparison>,
}

/// One-way ANOVA across `k` groups.
pub fn one_way_anova(groups: &[Vec<f64>]) -> Result<AnovaResult> {
    let k = groups.len();
    if k < 2 {
        return Err(anyhow!("One-way ANOVA needs at least 2 groups"));
    }
    if groups.iter().any(|g| g.is_empty()) {
        return Err(anyhow!("Groups must not be empty"));
    }
    let total_n: usize = groups.iter().map(|g| g.len()).sum();
    if total_n <= k {
        return Err(anyhow!("Not enough observations for ANOVA"));
    }

    let grand_mean = groups.iter().flat_map(|g| g.iter()).sum::<f64>() / total_n as f64;

    let mut ss_between = 0.0;
    let mut ss_within = 0.0;
    let mut group_means = Vec::with_capacity(k);
    let mut group_ns = Vec::with_capacity(k);
    for g in groups {
        let m = mean(g);
        let n = g.len() as f64;
        ss_between += n * (m - grand_mean).powi(2);
        ss_within += g.iter().map(|x| (x - m).powi(2)).sum::<f64>();
        group_means.push(m);
        group_ns.push(g.len());
    }

    let df_between = (k - 1) as f64;
    let df_within = (total_n - k) as f64;
    let ms_between = ss_between / df_between;
    let ms_within = ss_within / df_within;
    let f = if ms_within > 0.0 {
        ms_between / ms_within
    } else {
        f64::INFINITY
    };
    let p = f_upper_p(f, df_between, df_within);
    let ss_total = ss_between + ss_within;
    let eta_squared = if ss_total > 0.0 {
        ss_between / ss_total
    } else {
        0.0
    };

    // Bonferroni-corrected pairwise Welch-style comparisons.
    let num_pairs = (k * (k - 1) / 2) as f64;
    let mut pairwise = Vec::new();
    for i in 0..k {
        for j in (i + 1)..k {
            let ni = groups[i].len() as f64;
            let nj = groups[j].len() as f64;
            let vi = variance(&groups[i]);
            let vj = variance(&groups[j]);
            let se = (vi / ni + vj / nj).sqrt();
            let diff = group_means[i] - group_means[j];
            if se > 0.0 {
                let t = diff / se;
                let df = (vi / ni + vj / nj).powi(2)
                    / ((vi / ni).powi(2) / (ni - 1.0) + (vj / nj).powi(2) / (nj - 1.0));
                let raw_p = t_two_sided_p(t.abs(), df);
                let adj = (raw_p * num_pairs).min(1.0);
                pairwise.push(PairwiseComparison {
                    group_a: i,
                    group_b: j,
                    mean_diff: diff,
                    p_adjusted: adj,
                });
            } else {
                pairwise.push(PairwiseComparison {
                    group_a: i,
                    group_b: j,
                    mean_diff: diff,
                    p_adjusted: 1.0,
                });
            }
        }
    }

    Ok(AnovaResult {
        ss_between,
        ss_within,
        ss_total,
        df_between,
        df_within,
        ms_between,
        ms_within,
        f,
        p_value: p,
        eta_squared,
        group_means,
        group_ns,
        pairwise,
    })
}

// ---------------------------------------------------------------------------
// Two-way ANOVA (with replication)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TwoWayAnovaResult {
    pub ss_factor_a: f64,
    pub ss_factor_b: f64,
    pub ss_interaction: f64,
    pub ss_error: f64,
    pub ss_total: f64,
    pub df_a: f64,
    pub df_b: f64,
    pub df_interaction: f64,
    pub df_error: f64,
    pub ms_a: f64,
    pub ms_b: f64,
    pub ms_interaction: f64,
    pub ms_error: f64,
    pub f_a: f64,
    pub f_b: f64,
    pub f_interaction: f64,
    pub p_a: f64,
    pub p_b: f64,
    pub p_interaction: f64,
}

/// Two-way ANOVA with replication.
///
/// `data` is indexed `[row (factor A)][column (factor B)][replicate]` and every
/// cell must contain the same number of replicates.
pub fn two_way_anova(data: &[Vec<Vec<f64>>]) -> Result<TwoWayAnovaResult> {
    let a = data.len();
    if a < 2 {
        return Err(anyhow!("Need at least 2 levels of factor A"));
    }
    let b = data[0].len();
    if b < 2 {
        return Err(anyhow!("Need at least 2 levels of factor B"));
    }
    let n = data[0][0].len();
    if n < 2 {
        return Err(anyhow!("Two-way ANOVA needs replication (n >= 2)"));
    }
    for row in data {
        if row.len() != b {
            return Err(anyhow!("All rows must have the same number of columns"));
        }
        for cell in row {
            if cell.len() != n {
                return Err(anyhow!("All cells must have the same number of replicates"));
            }
        }
    }

    let na = a as f64;
    let nb = b as f64;
    let nn = n as f64;

    // Cell, row, column and grand means.
    let cell_mean = |i: usize, j: usize| -> f64 { mean(&data[i][j]) };
    let row_mean = |i: usize| -> f64 {
        let s: f64 = (0..b).map(|j| data[i][j].iter().sum::<f64>()).sum();
        s / (nb * nn)
    };
    let col_mean = |j: usize| -> f64 {
        let s: f64 = (0..a).map(|i| data[i][j].iter().sum::<f64>()).sum();
        s / (na * nn)
    };
    let grand: f64 = (0..a)
        .map(|i| (0..b).map(|j| data[i][j].iter().sum::<f64>()).sum::<f64>())
        .sum::<f64>()
        / (na * nb * nn);

    let mut ss_a = 0.0;
    let mut ss_b = 0.0;
    let mut ss_cells = 0.0; // between-cell (A + B + AB)
    let mut ss_error = 0.0;
    let mut ss_total = 0.0;

    for i in 0..a {
        ss_a += nb * nn * (row_mean(i) - grand).powi(2);
        for j in 0..b {
            let cm = cell_mean(i, j);
            ss_cells += nn * (cm - grand).powi(2);
            for v in &data[i][j] {
                ss_error += (v - cm).powi(2);
                ss_total += (v - grand).powi(2);
            }
        }
    }
    for j in 0..b {
        ss_b += na * nn * (col_mean(j) - grand).powi(2);
    }

    let ss_ab = ss_cells - ss_a - ss_b;

    let df_a = na - 1.0;
    let df_b = nb - 1.0;
    let df_ab = (na - 1.0) * (nb - 1.0);
    let df_error = na * nb * (nn - 1.0);

    let ms_a = ss_a / df_a;
    let ms_b = ss_b / df_b;
    let ms_ab = ss_ab / df_ab;
    let ms_error = ss_error / df_error;

    let f_a = ms_a / ms_error;
    let f_b = ms_b / ms_error;
    let f_ab = ms_ab / ms_error;

    Ok(TwoWayAnovaResult {
        ss_factor_a: ss_a,
        ss_factor_b: ss_b,
        ss_interaction: ss_ab,
        ss_error,
        ss_total,
        df_a,
        df_b,
        df_interaction: df_ab,
        df_error,
        ms_a,
        ms_b,
        ms_interaction: ms_ab,
        ms_error,
        f_a,
        f_b,
        f_interaction: f_ab,
        p_a: f_upper_p(f_a, df_a, df_error),
        p_b: f_upper_p(f_b, df_b, df_error),
        p_interaction: f_upper_p(f_ab, df_ab, df_error),
    })
}

// ---------------------------------------------------------------------------
// Non-parametric tests
// ---------------------------------------------------------------------------

/// Average ranks (ties get the mean of the ranks they span).
fn rank_with_ties(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| values[i].partial_cmp(&values[j]).unwrap_or(std::cmp::Ordering::Equal));
    let mut ranks = vec![0.0; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && values[order[j + 1]] == values[order[i]] {
            j += 1;
        }
        // ranks i+1 ..= j+1 (1-based), average
        let avg = ((i + 1) as f64 + (j + 1) as f64) / 2.0;
        for k in i..=j {
            ranks[order[k]] = avg;
        }
        i = j + 1;
    }
    ranks
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MannWhitneyResult {
    pub u_statistic: f64,
    pub z: f64,
    pub p_value: f64,
    pub median_a: f64,
    pub median_b: f64,
    pub n_a: usize,
    pub n_b: usize,
}

/// Mann-Whitney U test (normal approximation with continuity correction).
pub fn mann_whitney_u(a: &[f64], b: &[f64]) -> Result<MannWhitneyResult> {
    if a.is_empty() || b.is_empty() {
        return Err(anyhow!("Both groups must be non-empty"));
    }
    let n1 = a.len() as f64;
    let n2 = b.len() as f64;

    let mut combined: Vec<f64> = Vec::with_capacity(a.len() + b.len());
    combined.extend_from_slice(a);
    combined.extend_from_slice(b);
    let ranks = rank_with_ties(&combined);

    let r1: f64 = ranks[..a.len()].iter().sum();
    let u1 = r1 - n1 * (n1 + 1.0) / 2.0;
    let u2 = n1 * n2 - u1;
    let u = u1.min(u2);

    // Tie correction for the variance.
    let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for v in &combined {
        let key = (v * 1e9).round() as i64;
        *counts.entry(key).or_insert(0) += 1;
    }
    let tie_sum: f64 = counts
        .values()
        .map(|&t| {
            let t = t as f64;
            t * t * t - t
        })
        .sum();
    let n = n1 + n2;
    let var = (n1 * n2 / 12.0) * ((n + 1.0) - tie_sum / (n * (n - 1.0)));
    let mean_u = n1 * n2 / 2.0;
    // Continuity correction of 0.5 towards the mean.
    let cc = if u < mean_u { 0.5 } else { -0.5 };
    let z = if var > 0.0 {
        (u - mean_u + cc) / var.sqrt()
    } else {
        0.0
    };
    let p = 2.0 * normal_sf(z.abs());

    Ok(MannWhitneyResult {
        u_statistic: u,
        z,
        p_value: p,
        median_a: calculate_descriptive(a).map(|s| s.median).unwrap_or(f64::NAN),
        median_b: calculate_descriptive(b).map(|s| s.median).unwrap_or(f64::NAN),
        n_a: a.len(),
        n_b: b.len(),
    })
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WilcoxonResult {
    pub w_plus: f64,
    pub w_minus: f64,
    pub z: f64,
    pub p_value: f64,
    pub n_effective: usize,
}

/// Wilcoxon signed-rank test (normal approximation).
pub fn wilcoxon_signed_rank(a: &[f64], b: &[f64]) -> Result<WilcoxonResult> {
    if a.len() != b.len() {
        return Err(anyhow!("Paired samples must have the same length"));
    }
    let diffs: Vec<f64> = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| x - y)
        .filter(|d| d.abs() > 0.0)
        .collect();
    let n = diffs.len();
    if n < 1 {
        return Err(anyhow!("All differences are zero"));
    }
    let abs: Vec<f64> = diffs.iter().map(|d| d.abs()).collect();
    let ranks = rank_with_ties(&abs);

    let mut w_plus = 0.0;
    let mut w_minus = 0.0;
    for (i, &d) in diffs.iter().enumerate() {
        if d > 0.0 {
            w_plus += ranks[i];
        } else {
            w_minus += ranks[i];
        }
    }
    let w = w_plus.min(w_minus);
    let nf = n as f64;
    let mean_w = nf * (nf + 1.0) / 4.0;
    let sd_w = (nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0).sqrt();
    let cc = if w < mean_w { 0.5 } else { -0.5 };
    let z = if sd_w > 0.0 {
        (w - mean_w + cc) / sd_w
    } else {
        0.0
    };
    let p = 2.0 * normal_sf(z.abs());

    Ok(WilcoxonResult {
        w_plus,
        w_minus,
        z,
        p_value: p,
        n_effective: n,
    })
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KruskalWallisResult {
    pub h: f64,
    pub df: f64,
    pub p_value: f64,
}

/// Kruskal-Wallis one-way analysis of variance by ranks.
pub fn kruskal_wallis(groups: &[Vec<f64>]) -> Result<KruskalWallisResult> {
    let k = groups.len();
    if k < 2 {
        return Err(anyhow!("Kruskal-Wallis needs at least 2 groups"));
    }
    if groups.iter().any(|g| g.is_empty()) {
        return Err(anyhow!("Groups must not be empty"));
    }
    let mut combined: Vec<f64> = Vec::new();
    let mut offsets = Vec::new();
    for g in groups {
        offsets.push(combined.len());
        combined.extend_from_slice(g);
    }
    let n = combined.len();
    let ranks = rank_with_ties(&combined);

    let mut h = 0.0;
    for (gi, g) in groups.iter().enumerate() {
        let start = offsets[gi];
        let end = start + g.len();
        let r_sum: f64 = ranks[start..end].iter().sum();
        let ni = g.len() as f64;
        h += r_sum * r_sum / ni;
    }
    h = 12.0 / (n as f64 * (n as f64 + 1.0)) * h - 3.0 * (n as f64 + 1.0);

    // Tie correction.
    let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for v in &combined {
        let key = (v * 1e9).round() as i64;
        *counts.entry(key).or_insert(0) += 1;
    }
    let tie_sum: f64 = counts
        .values()
        .map(|&t| {
            let t = t as f64;
            t * t * t - t
        })
        .sum();
    if tie_sum > 0.0 {
        let nf = n as f64;
        let corr = 1.0 - tie_sum / (nf * nf * nf - nf);
        if corr > 0.0 {
            h /= corr;
        }
    }

    let df = (k - 1) as f64;
    Ok(KruskalWallisResult {
        h,
        df,
        p_value: chisq_upper_p(h, df),
    })
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FriedmanResult {
    pub chi_square: f64,
    pub df: f64,
    pub p_value: f64,
}
/// Friedman test for repeated-measures data. `blocks[block][treatment]`.
pub fn friedman(blocks: &[Vec<f64>]) -> Result<FriedmanResult> {
    if blocks.is_empty() {
        return Err(anyhow!("No data"));
    }
    let k = blocks[0].len();
    if k < 2 {
        return Err(anyhow!("Friedman needs at least 2 treatments"));
    }
    if blocks.iter().any(|b| b.len() != k) {
        return Err(anyhow!("All blocks must have the same number of treatments"));
    }
    let n = blocks.len();

    // Rank within each block.
    let mut rank_sums = vec![0.0; k];
    for block in blocks {
        let ranks = rank_with_ties(block);
        for (j, r) in ranks.iter().enumerate() {
            rank_sums[j] += r;
        }
    }
    let nf = n as f64;
    let kf = k as f64;
    let sum_sq: f64 = rank_sums.iter().map(|r| r * r).sum();
    let chi2 = 12.0 / (nf * kf * (kf + 1.0)) * sum_sq - 3.0 * nf * (kf + 1.0);
    let df = kf - 1.0;

    Ok(FriedmanResult {
        chi_square: chi2,
        df,
        p_value: chisq_upper_p(chi2, df),
    })
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChiSquareResult {
    pub chi_square: f64,
    pub df: f64,
    pub p_value: f64,
    /// Expected counts under independence.
    pub expected: Vec<Vec<f64>>,
    /// Cramer's V effect size.
    pub cramers_v: f64,
}

/// Chi-square test of independence for a contingency table.
pub fn chi_square_independence(counts: &[Vec<u64>]) -> Result<ChiSquareResult> {
    let rows = counts.len();
    if rows < 2 {
        return Err(anyhow!("A contingency table needs at least 2 rows"));
    }
    let cols = counts[0].len();
    if cols < 2 {
        return Err(anyhow!("A contingency table needs at least 2 columns"));
    }
    if counts.iter().any(|r| r.len() != cols) {
        return Err(anyhow!("All rows must have the same number of columns"));
    }

    let total: f64 = counts.iter().flat_map(|r| r.iter()).map(|v| *v as f64).sum();
    if total <= 0.0 {
        return Err(anyhow!("The contingency table is empty"));
    }
    let row_sums: Vec<f64> = counts.iter().map(|r| r.iter().map(|v| *v as f64).sum()).collect();
    let col_sums: Vec<f64> = (0..cols)
        .map(|j| counts.iter().map(|r| r[j] as f64).sum())
        .collect();

    let mut chi2 = 0.0;
    let mut expected = vec![vec![0.0; cols]; rows];
    for i in 0..rows {
        for j in 0..cols {
            let e = row_sums[i] * col_sums[j] / total;
            expected[i][j] = e;
            if e > 0.0 {
                let o = counts[i][j] as f64;
                chi2 += (o - e).powi(2) / e;
            }
        }
    }

    let df = ((rows - 1) * (cols - 1)) as f64;
    let k = rows.min(cols) as f64;
    let cramers_v = if total > 0.0 && k > 1.0 {
        (chi2 / (total * (k - 1.0))).sqrt()
    } else {
        0.0
    };

    Ok(ChiSquareResult {
        chi_square: chi2,
        df,
        p_value: chisq_upper_p(chi2, df),
        expected,
        cramers_v,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chi_square_detects_association() {
        // Strong association: rows and columns move together.
        let table = vec![vec![90u64, 10], vec![10, 90]];
        let r = chi_square_independence(&table).unwrap();
        assert!(r.chi_square > 100.0, "chi2 {}", r.chi_square);
        assert!((r.df - 1.0).abs() < 1e-9);
        assert!(r.p_value < 1e-6);
        // Expected counts are all 50 for a balanced 2x2.
        for row in &r.expected {
            for e in row {
                assert!((e - 50.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn chi_square_zero_when_independent() {
        let table = vec![vec![50u64, 50], vec![50, 50]];
        let r = chi_square_independence(&table).unwrap();
        assert!(r.chi_square < 1e-9);
        assert!(r.p_value > 0.99);
    }

    #[test]
    fn chi_square_rejects_bad_tables() {
        assert!(chi_square_independence(&[vec![1, 2]]).is_err()); // single row
        assert!(chi_square_independence(&[vec![0, 0], vec![0, 0]]).is_err()); // empty
    }

    #[test]
    fn unpaired_t_known() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [6.0, 7.0, 8.0, 9.0, 10.0];
        let r = t_test_unpaired(&a, &b, false).unwrap();
        // means 3 and 8, diff -5, pooled SD sqrt(2.5), SE 1
        assert!((r.mean_diff + 5.0).abs() < 1e-9);
        assert!((r.t + 5.0).abs() < 1e-6, "t {}", r.t);
        assert!((r.df - 8.0).abs() < 1e-9);
        assert!(r.p_value < 0.01, "p {}", r.p_value);
        assert!(r.ci95_diff.0 < r.ci95_diff.1);
    }

    #[test]
    fn welch_t_unequal_variance() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [2.0, 4.0, 6.0, 8.0, 10.0];
        let r = t_test_unpaired(&a, &b, true).unwrap();
        assert_eq!(r.kind, TTestKind::Welch);
        // Welch df is strictly smaller than the pooled df (8).
        assert!(r.df < 8.0 && r.df > 2.0, "df {}", r.df);
        assert!(r.p_value > 0.0 && r.p_value < 1.0);
    }

    #[test]
    fn paired_t_known() {
        let a = [10.0, 12.0, 14.0, 16.0, 18.0];
        let b = [9.0, 11.0, 13.0, 15.0, 17.0];
        let r = t_test_paired(&a, &b).unwrap();
        assert!((r.mean_diff - 1.0).abs() < 1e-9);
        assert!(r.p_value < 1e-6);
    }

    #[test]
    fn one_sample_against_zero() {
        let d = [1.0, 2.0, 3.0, 4.0, 5.0];
        let r = t_test_one_sample(&d, 0.0).unwrap();
        assert!((r.mean_a - 3.0).abs() < 1e-9);
        assert!(r.p_value < 0.05);
    }

    #[test]
    fn anova_detects_difference() {
        let g = vec![
            vec![1.0, 1.1, 1.2],
            vec![5.0, 5.1, 5.2],
            vec![9.0, 9.1, 9.2],
        ];
        let r = one_way_anova(&g).unwrap();
        assert!(r.f > 100.0);
        assert!(r.p_value < 1e-4);
        assert_eq!(r.pairwise.len(), 3);
    }

    #[test]
    fn two_way_runs() {
        let data = vec![
            vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            vec![vec![5.0, 6.0], vec![7.0, 8.0]],
        ];
        let r = two_way_anova(&data).unwrap();
        assert!(r.ss_total > 0.0);
        assert!(r.p_a >= 0.0 && r.p_a <= 1.0);
    }

    #[test]
    fn mann_whitney_separated() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [10.0, 11.0, 12.0, 13.0];
        let r = mann_whitney_u(&a, &b).unwrap();
        assert!(r.p_value < 0.05);
    }

    #[test]
    fn wilcoxon_separated() {
        let a = [10.0, 12.0, 14.0, 16.0, 18.0];
        let b = [1.0, 2.0, 3.0, 4.0, 5.0];
        let r = wilcoxon_signed_rank(&a, &b).unwrap();
        assert!(r.p_value < 0.1);
    }

    #[test]
    fn kruskal_detects() {
        let g = vec![vec![1.0, 2.0, 3.0], vec![10.0, 11.0, 12.0], vec![20.0, 21.0, 22.0]];
        let r = kruskal_wallis(&g).unwrap();
        assert!(r.p_value < 0.05);
    }

    #[test]
    fn friedman_runs() {
        let blocks = vec![
            vec![1.0, 2.0, 3.0],
            vec![2.0, 3.0, 1.0],
            vec![1.5, 2.5, 3.5],
        ];
        let r = friedman(&blocks).unwrap();
        assert!(r.p_value >= 0.0 && r.p_value <= 1.0);
    }
}
