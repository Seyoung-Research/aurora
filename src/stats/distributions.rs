//! Statistical distributions used to turn test statistics into p-values and to
//! invert distributions (confidence limits).
//!
//! Everything here is dependency-free numerical code (no `ndarray`, no external
//! stats crate) so the engine is self-contained and deterministic.
//!
//! Implemented:
//! - Normal  : CDF / SF / inverse CDF
//! - Student t: CDF (via regularized incomplete beta) / inverse CDF (bisection)
//! - F       : CDF / inverse CDF
//! - Chi-square: CDF (via regularized incomplete gamma) / inverse CDF
//!
//! The regularized incomplete beta and gamma are the classic
//! continued-fraction implementations, which are accurate across the
//! distributions' whole ranges.

/// Two-sided normal p-value (i.e. `2 * P(Z >= |z|)`).
pub fn normal_two_sided_p(z: f64) -> f64 {
    2.0 * normal_sf(z.abs())
}

// ---------------------------------------------------------------------------
// erf / normal
// ---------------------------------------------------------------------------

/// Error function using the Abramowitz & Stegun 7.1.26 approximation
/// (max absolute error ~1.5e-7), which is more than enough for p-values and CIs.
pub fn erf(x: f64) -> f64 {
    let sign = x.signum();
    let x = x.abs();
    // Abramowitz & Stegun 7.1.26, max |err| ~ 1.5e-7.
    const P: f64 = 0.3275911;
    const A: f64 = 0.254829592;
    const B: f64 = -0.284496736;
    const C: f64 = 1.421413741;
    const D: f64 = -1.453152027;
    const E: f64 = 1.061405429;
    let t = 1.0 / (1.0 + P * x);
    let poly = ((((E * t + D) * t + C) * t + B) * t + A) * t;
    sign * (1.0 - poly * (-x * x).exp())
}

/// Standard-normal cumulative distribution function.
pub fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// Standard-normal survival function.
pub fn normal_sf(x: f64) -> f64 {
    1.0 - normal_cdf(x)
}

/// Inverse standard-normal CDF (`ppf`) via bisection on the accurate CDF.
pub fn normal_ppf(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let (mut lo, mut hi) = (-40.0_f64, 40.0_f64);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if normal_cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

// ---------------------------------------------------------------------------
// Incomplete beta / gamma
// ---------------------------------------------------------------------------

/// Regularized incomplete beta function `I_x(a, b)`.
///
/// Uses the hypergeometric power series in the region where it converges
/// quickly, and the symmetry relation `I_x(a,b) = 1 - I_{1-x}(b,a)` otherwise.
pub fn regularized_incomplete_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    if !a.is_finite() || !b.is_finite() {
        return 0.0;
    }

    if x < (a + 1.0) / (a + b + 2.0) {
        incomplete_beta_series(a, b, x)
    } else {
        1.0 - incomplete_beta_series(b, a, 1.0 - x)
    }
}

/// Continued fraction for the incomplete beta function.
///
/// Note: for small `b` (e.g. the t-distribution's `b = 1/2`) this Lentz-style
/// continued fraction converges very slowly, so [`regularized_incomplete_beta`]
/// prefers the power series [`incomplete_beta_series`].
#[allow(dead_code)]
fn incomplete_beta_cf(a: f64, b: f64, x: f64) -> f64 {
    const MAX_ITER: u32 = 300;
    const EPS: f64 = 1e-13;
    const FPMIN: f64 = 1e-300;
    let guard = |v: &mut f64| {
        if v.abs() < FPMIN {
            *v = FPMIN;
        }
    };

    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;

    let mut c = 1.0;
    let mut d = 1.0 - (qab * x) / qap;
    guard(&mut d);
    d = 1.0 / d;
    let mut h = d;

    for m in 1..=MAX_ITER {
        let m1 = m as f64;
        let m2 = 2.0 * m1;
        // even term
        let aa = (m1 * (b - m1) * x) / ((qab + m2 - 1.0) * (qap + m2 - 1.0));
        d = 1.0 + aa * d;
        guard(&mut d);
        c = 1.0 + aa / c;
        guard(&mut c);
        d = 1.0 / d;
        h *= d * c;
        // odd term
        let aa = -((qab + m1) * (a + m1) * x) / ((qap + m1) * (qam + m1));
        d = 1.0 + aa * d;
        guard(&mut d);
        c = 1.0 + aa / c;
        guard(&mut c);
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() <= EPS {
            break;
        }
    }
    h
}

/// `ln B(a,b) = ln Γ(a) + ln Γ(b) - ln Γ(a+b)`.
fn ln_beta(a: f64, b: f64) -> f64 {
    ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b)
}

/// Incomplete beta via the hypergeometric power series
/// `I_x(a,b) = x^a / (a B(a,b)) * 2F1(a, 1-b; a+1; x)`,
/// valid (and rapidly converging) for `x < (a+1)/(a+b+2)`.
fn incomplete_beta_series(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    // ln( x^a / (a B(a,b)) )
    let ln_front = a * x.ln() - ln_beta(a, b) - a.ln();
    let mut term = 1.0_f64;
    let mut sum = 1.0_f64;
    // term_{n+1} / term_n = ((a+n)(1-b+n)) / ((a+1+n)(n+1)) * x
    for n in 0..1000 {
        let nf = n as f64;
        let ratio = ((a + nf) * (1.0 - b + nf)) / ((a + 1.0 + nf) * (nf + 1.0)) * x;
        term *= ratio;
        sum += term;
        if term.abs() <= sum.abs() * 1e-16 {
            break;
        }
    }
    (ln_front.exp() * sum).clamp(0.0, 1.0)
}

/// Lower regularized incomplete gamma `P(a, x) = γ(a, x) / Γ(a)`.
pub fn regularized_lower_gamma(a: f64, x: f64) -> f64 {
    if x <= 0.0 || a <= 0.0 {
        return 0.0;
    }
    if !x.is_finite() {
        return 1.0;
    }
    if x < a + 1.0 {
        gamma_series(a, x)
    } else {
        1.0 - gamma_cf(a, x)
    }
}

/// Natural log of the Gamma function (Lanczos approximation, |err| < ~1e-12).
pub fn ln_gamma(x: f64) -> f64 {
    const G: [f64; 9] = [
        676.5203681218851, -1259.1392167224028, 771.32342877765313,
        -176.61502916214059, 12.507343278686905, -0.13857109526572012,
        9.9843695780195716e-6, 1.5056327351493116e-7, 0.0,
    ];
    if x < 0.5 {
        // Euler's reflection formula.
        std::f64::consts::PI.ln()
            - (std::f64::consts::PI * x).sin().ln()
            - ln_gamma(1.0 - x)
    } else {
        let x = x - 1.0;
        let mut a = 0.99999999999980993;
        for (i, &coeff) in G.iter().enumerate() {
            a += coeff / (x + 1.0 + i as f64);
        }
        let t = x + 7.5;
        // 0.5*ln(2pi) is the Lanczos normalization constant.
        (x + 0.5) * t.ln() - t + a.ln() + 0.5 * (2.0 * std::f64::consts::PI).ln()
    }
}

fn gamma_series(a: f64, x: f64) -> f64 {
    const MAX_ITER: u32 = 500;
    const EPS: f64 = 1e-15;
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    for _n in 1..MAX_ITER {
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if del.abs() < sum.abs() * EPS {
            break;
        }
    }
    let ln = a * x.ln() - x - ln_gamma(a);
    (sum * ln.exp()).clamp(0.0, 1.0)
}

fn gamma_cf(a: f64, x: f64) -> f64 {
    const MAX_ITER: u32 = 500;
    const EPS: f64 = 1e-15;
    const FPMIN: f64 = 1e-300;

    let clamp = |v: f64| if v.abs() < FPMIN { FPMIN } else { v };

    let mut b = clamp(x + 1.0 - a);
    let mut c = 1.0 / FPMIN;
    let mut d = clamp(1.0 / b);
    let mut h = d;

    for i in 1..MAX_ITER {
        // Numerical Recipes gammap: `an = -i * (i - a)` (the leading sign matters).
        let ai = -((i as f64) * (i as f64 - a));
        b = b + 2.0;
        d = ai * d + b;
        d = clamp(d);
        c = b + ai / c;
        c = clamp(c);
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    let ln = a * x.ln() - x - ln_gamma(a);
    (h * ln.exp()).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// t, F, chi-square
// ---------------------------------------------------------------------------

/// Two-tailed p-value of a Student-t statistic with `df` degrees of freedom.
pub fn t_two_sided_p(t: f64, df: f64) -> f64 {
    if !df.is_finite() || df <= 0.0 {
        return 0.0;
    }
    let x = df / (df + t * t);
    regularized_incomplete_beta(df / 2.0, 0.5, x).clamp(0.0, 1.0)
}

/// One-tailed p-value `P(T >= t)`.
pub fn t_upper_p(t: f64, df: f64) -> f64 {
    if t.is_nan() || df <= 0.0 {
        return 0.5;
    }
    let x = (df / (df + t * t)).clamp(0.0, 1.0);
    0.5 * regularized_incomplete_beta(df / 2.0, 0.5, x)
}

/// Inverse of the t CDF: `p` in (0,1) -> t.
pub fn t_ppf(p: f64, df: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let cdf = |t: f64| -> f64 {
        let x = (df / (df + t * t)).clamp(0.0, 1.0);
        let ib = regularized_incomplete_beta(df / 2.0, 0.5, x);
        if t >= 0.0 {
            1.0 - 0.5 * ib
        } else {
            0.5 * ib
        }
    };
    let (mut lo, mut hi) = (-1000.0, 1000.0);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// CDF of the F distribution.
pub fn f_cdf(y: f64, d1: f64, d2: f64) -> f64 {
    if y <= 0.0 {
        return 0.0;
    }
    let x = (d1 * y) / (d1 * y + d2);
    regularized_incomplete_beta(d1 / 2.0, d2 / 2.0, x)
}

/// Survival (upper tail) of the F distribution.
pub fn f_upper_p(y: f64, d1: f64, d2: f64) -> f64 {
    1.0 - f_cdf(y, d1, d2)
}

/// Inverse F CDF.
pub fn f_ppf(p: f64, d1: f64, d2: f64) -> f64 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let (mut lo, mut hi) = (0.0, 1000.0_f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if f_cdf(mid, d1, d2) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Chi-square CDF with `k` degrees of freedom.
pub fn chisq_cdf(x: f64, k: f64) -> f64 {
    regularized_lower_gamma(k / 2.0, x / 2.0)
}

/// Chi-square upper tail.
pub fn chisq_upper_p(x: f64, k: f64) -> f64 {
    1.0 - chisq_cdf(x, k)
}

/// Inverse chi-square CDF.
pub fn chisq_ppf(p: f64, k: f64) -> f64 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let (mut lo, mut hi) = (0.0, 2.0 * k + 50.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if chisq_cdf(mid, k) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_cdf_values() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-9);
        assert!((normal_cdf(1.0) - 0.84134474607).abs() < 1e-5);
        assert!((normal_cdf(1.96) - 0.975).abs() < 1e-3);
        assert!((normal_cdf(-1.0) - 0.15865525393).abs() < 1e-5);
    }

    #[test]
    fn normal_ppf_roundtrip() {
        for p in [0.025, 0.1, 0.1587, 0.5, 0.8413, 0.95, 0.975, 0.999] {
            let z = normal_ppf(p);
            let back = normal_cdf(z);
            assert!((back - p).abs() < 1e-3, "p={p} z={z} cdf={back}");
        }
        assert!((normal_ppf(0.975) - 1.9599639845).abs() < 1e-3);
    }

    #[test]
    fn incomplete_beta_endpoints() {
        assert!((regularized_incomplete_beta(2.0, 3.0, 0.0) - 0.0).abs() < 1e-9);
        assert!((regularized_incomplete_beta(2.0, 3.0, 1.0) - 1.0).abs() < 1e-9);
        // Symmetry: I_x(a,b) + I_{1-x}(b,a) = 1
        let x = 0.3;
        let s = regularized_incomplete_beta(2.3, 1.7, x) + regularized_incomplete_beta(1.7, 2.3, 1.0 - x);
        assert!((s - 1.0).abs() < 1e-6, "sum={s}");
    }

    #[test]
    fn t_dist() {
        // Two-tailed p at the df=30, alpha=0.05 critical value (~2.0423) is 0.05.
        let p_crit = t_two_sided_p(2.0423, 30.0);
        assert!((p_crit - 0.05).abs() < 2e-3, "got {p_crit}");
        // Known inverse.
        let q = t_ppf(0.975, 10.0);
        assert!((q - 2.2281).abs() < 1e-3, "got {q}");
        // large df → tends to normal
        assert!((t_ppf(0.975, 1000.0) - 1.96).abs() < 1e-2);
    }

    #[test]
    fn f_dist() {
        let c = f_ppf(0.95, 3.0, 20.0);
        assert!((c - 3.1000).abs() < 0.05, "got {c}");
    }

    #[test]
    fn chisq() {
        let q = chisq_ppf(0.95, 3.0);
        assert!((q - 7.8147).abs() < 1e-2, "got {q}");
        // Analytic: P(3/2, 3/2) = erf(sqrt(1.5)) - (2 sqrt(1.5)/sqrt(pi)) e^{-1.5} = 0.608374...
        let c = chisq_cdf(3.0, 3.0);
        assert!((c - 0.60837).abs() < 1e-4, "got {c}");
    }
}
