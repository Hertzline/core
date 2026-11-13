//! Median / IQR helpers for pass aggregates (AC-2.2).

/// Median and spread of a set of per-pass scalar metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricSpread {
    pub median: f64,
    /// Interquartile range (Q3 - Q1). Zero when fewer than 2 values.
    pub iqr: f64,
    pub min: f64,
    pub max: f64,
    pub sample_count: usize,
}

impl MetricSpread {
    /// Aggregate sorted copy of `values`. Empty -> `None`.
    pub fn from_values(values: &[f64]) -> Option<Self> {
        let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
        if sorted.is_empty() {
            return None;
        }
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let median = median_sorted(&sorted);
        let min = sorted[0];
        let max = sorted[n - 1];
        let iqr = if n >= 2 {
            let q1 = percentile_sorted(&sorted, 0.25);
            let q3 = percentile_sorted(&sorted, 0.75);
            q3 - q1
        } else {
            0.0
        };
        Some(Self {
            median,
            iqr,
            min,
            max,
            sample_count: n,
        })
    }
}

/// Median of finite values (unsorted input).
pub fn median_f64(values: &[f64]) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(median_sorted(&sorted))
}

fn median_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Linear interpolation percentile on a sorted ascending slice. `p` in [0, 1].
fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let p = p.clamp(0.0, 1.0);
    let idx = p * ((n - 1) as f64);
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let w = idx - (lo as f64);
    sorted[lo] * (1.0 - w) + sorted[hi] * w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_odd_even() {
        assert!((median_f64(&[1.0, 3.0, 2.0]).unwrap() - 2.0).abs() < 1e-12);
        assert!((median_f64(&[1.0, 2.0, 3.0, 4.0]).unwrap() - 2.5).abs() < 1e-12);
    }

    #[test]
    fn spread_min_max_iqr() {
        // mock: three pass avg FPS values
        let s = MetricSpread::from_values(&[60.0, 62.0, 64.0]).expect("spread");
        assert!((s.median - 62.0).abs() < 1e-12);
        assert!((s.min - 60.0).abs() < 1e-12);
        assert!((s.max - 64.0).abs() < 1e-12);
        assert!(s.iqr >= 0.0);
        assert_eq!(s.sample_count, 3);
    }

    #[test]
    fn empty_rejected() {
        assert!(MetricSpread::from_values(&[]).is_none());
        assert!(median_f64(&[]).is_none());
    }
}
