//! Axis scales (linear / log10) and "nice" tick generation.

use serde::{Deserialize, Serialize};

/// How an axis maps data values to positions.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleKind {
    Linear,
    /// Base-10 logarithm. Values <= 0 are not drawn.
    Log10,
}

impl Default for ScaleKind {
    fn default() -> Self {
        ScaleKind::Linear
    }
}

/// Description of one axis.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Axis {
    pub label: String,
    pub scale: ScaleKind,
    /// Fixed data range; `None` means "choose from the data".
    pub range: Option<(f64, f64)>,
    /// Approximate number of ticks to aim for.
    pub tick_count: usize,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            label: String::new(),
            scale: ScaleKind::Linear,
            range: None,
            tick_count: 6,
        }
    }
}

impl Axis {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }

    pub fn log(mut self) -> Self {
        self.scale = ScaleKind::Log10;
        self
    }

    pub fn with_range(mut self, lo: f64, hi: f64) -> Self {
        self.range = Some((lo, hi));
        self
    }
}

/// Transform a raw value into "linear drawing space" for the given scale.
///
/// Returns `None` when the value cannot be represented (e.g. `<= 0` on a log
/// axis).
pub fn transform(v: f64, scale: ScaleKind) -> Option<f64> {
    if !v.is_finite() {
        return None;
    }
    match scale {
        ScaleKind::Linear => Some(v),
        ScaleKind::Log10 => {
            if v > 0.0 {
                Some(v.log10())
            } else {
                None
            }
        }
    }
}

/// Inverse of [`transform`].
pub fn inverse(t: f64, scale: ScaleKind) -> f64 {
    match scale {
        ScaleKind::Linear => t,
        ScaleKind::Log10 => 10.0_f64.powf(t),
    }
}

/// Step size for the 1-2-5 "nice number" progression.
fn nice_step(rough: f64) -> f64 {
    if rough <= 0.0 || !rough.is_finite() {
        return 1.0;
    }
    let exp = rough.log10().floor();
    let base = 10.0_f64.powf(exp);
    let frac = rough / base;
    let mult = if frac <= 1.0 {
        1.0
    } else if frac <= 2.0 {
        2.0
    } else if frac <= 5.0 {
        5.0
    } else {
        10.0
    };
    mult * base
}

/// Choose a "nice" data range that contains `[min, max]`.
///
/// On a log axis the result is expanded to whole decades when the span is
/// large, otherwise to nice mantissa multiples.
pub fn nice_range(min: f64, max: f64, scale: ScaleKind, ticks: usize) -> (f64, f64) {
    let ticks = ticks.clamp(2, 20);
    match scale {
        ScaleKind::Linear => {
            if !min.is_finite() || !max.is_finite() {
                return (0.0, 1.0);
            }
            if (max - min).abs() < f64::EPSILON {
                // Degenerate range: pad it so the point is not on the frame.
                let pad = if min.abs() > 0.0 { min.abs() * 0.5 } else { 1.0 };
                return (min - pad, max + pad);
            }
            let step = nice_step((max - min) / ticks as f64);
            let lo = (min / step).floor() * step;
            let hi = (max / step).ceil() * step;
            (lo, hi)
        }
        ScaleKind::Log10 => {
            let lo = if min > 0.0 { min } else { 1e-3 };
            let hi = if max > lo { max } else { lo * 10.0 };
            let llo = lo.log10();
            let lhi = hi.log10();
            let span = lhi - llo;
            if span >= ticks as f64 {
                // One tick per decade.
                (10.0_f64.powf(llo.floor()), 10.0_f64.powf(lhi.ceil()))
            } else {
                let step = nice_step(span / ticks as f64);
                let a = (llo / step).floor() * step;
                let b = (lhi / step).ceil() * step;
                (10.0_f64.powf(a), 10.0_f64.powf(b))
            }
        }
    }
}

/// Human-readable tick label (avoids `1e-7` style output for common values).
pub fn format_tick(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let a = v.abs();
    if a >= 1e5 || a < 1e-4 {
        let s = format!("{v:.2e}");
        // Trim trailing zeros in the mantissa: 1.00e5 -> 1e5
        if let Some((m, e)) = s.split_once('e') {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            return format!("{m}e{e}");
        }
        s
    } else {
        // Up to 6 significant-ish decimals, trailing zeros trimmed.
        let mut s = format!("{v:.6}");
        if s.contains('.') {
            s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        s
    }
}

/// Tick positions for the given range, **in drawing space**.
///
/// The range must already be transformed by [`transform`]: on a log axis the
/// bounds are `log10(value)` and the returned positions are `log10(value)` too.
/// This keeps tick placement independent of the mapping used by the renderer.
pub fn ticks(range: (f64, f64), scale: ScaleKind, count: usize) -> Vec<f64> {
    let count = count.clamp(2, 20);
    let (lo, hi) = range;
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return vec![lo];
    }
    match scale {
        ScaleKind::Linear => {
            // For very wide ranges a linear step can be a large number; the
            // 1-2-5 rule keeps labels readable in either direction.
            let step = nice_step((hi - lo) / count as f64);
            let first = (lo / step).ceil() * step;
            let mut out = Vec::new();
            let mut v = first;
            for _ in 0..200 {
                if v > hi + step * 1e-6 {
                    break;
                }
                out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
                v += step;
            }
            out
        }
        ScaleKind::Log10 => {
            let span = hi - lo;
            let mut out = Vec::new();
            if span >= 1.9 {
                // Whole decades (readable when several decades are visible).
                let mut e = lo.ceil();
                for _ in 0..200 {
                    if e > hi + 1e-9 {
                        break;
                    }
                    out.push(e);
                    e += 1.0;
                }
            } else {
                // 1/2/5 mantissas so a narrow log axis is not too sparse.
                let mants = [1.0_f64, 2.0, 5.0];
                let base_exp = lo.floor();
                for d in 0..40 {
                    let e = base_exp + d as f64;
                    for m in mants {
                        let t = e + m.log10();
                        if t >= lo - 1e-9 && t <= hi + 1e-9 {
                            out.push(t);
                        }
                    }
                    if e > hi + 1.0 {
                        break;
                    }
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_ticks_are_nice() {
        let t = ticks((0.0, 10.0), ScaleKind::Linear, 6);
        assert!(t.len() >= 5);
        assert!((t[0] - 0.0).abs() < 1e-12);
        // step should be 2
        assert!((t[1] - t[0] - 2.0).abs() < 1e-9, "ticks {t:?}");
    }

    #[test]
    fn nice_range_covers_data() {
        let (lo, hi) = nice_range(0.3, 9.7, ScaleKind::Linear, 6);
        assert!(lo <= 0.3 && hi >= 9.7, "{lo} {hi}");
    }

    #[test]
    fn log_ticks_are_decades() {
        // Drawing space for 0.1 .. 1000 is -1 .. 3.
        let t = ticks((-1.0, 3.0), ScaleKind::Log10, 6);
        assert_eq!(t.len(), 5, "ticks {t:?}");
        assert!((t[0] - -1.0).abs() < 1e-12);
        assert!((t[4] - 3.0).abs() < 1e-9);
        // Labels come back as data values.
        assert!((inverse(t[0], ScaleKind::Log10) - 0.1).abs() < 1e-12);
        assert!((inverse(t[4], ScaleKind::Log10) - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn narrow_log_ticks_use_mantissas() {
        // 1 .. 10 is 0 .. 1 in drawing space.
        let t = ticks((0.0, 1.0), ScaleKind::Log10, 6);
        let vals: Vec<f64> = t.iter().map(|v| inverse(*v, ScaleKind::Log10)).collect();
        assert!(vals.len() >= 4, "vals {vals:?}");
        assert!(vals.iter().any(|v| (v - 5.0).abs() < 1e-9), "{vals:?}");
    }

    #[test]
    fn log_transform_rejects_nonpositive() {
        assert!(transform(0.0, ScaleKind::Log10).is_none());
        assert!(transform(-1.0, ScaleKind::Log10).is_none());
        assert!((transform(100.0, ScaleKind::Log10).unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn format_ticks_readable() {
        assert_eq!(format_tick(0.0), "0");
        assert_eq!(format_tick(2.5), "2.5");
        assert_eq!(format_tick(1e6), "1e6");
    }
}
