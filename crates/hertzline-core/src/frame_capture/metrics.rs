//! Frame-time statistics from an ordered list of frame durations (milliseconds).
//!
//! Used once a capture backend (or mock) supplies samples. Mocks must be labeled.

/// Aggregate stats for AC-1.1: average FPS, 1% low, 0.1% low, frame-time variance.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameTimeStats {
    pub sample_count: usize,
    pub avg_fps: f64,
    pub low_1pct_fps: f64,
    pub low_0_1pct_fps: f64,
    pub frame_time_variance_ms2: f64,
    pub avg_frame_time_ms: f64,
}

/// Compute stats from frame times in milliseconds.
///
/// Returns `None` when there are fewer than 2 positive samples (cannot form FPS lows).
pub fn compute_stats_ms(frame_times_ms: &[f64]) -> Option<FrameTimeStats> {
    let samples: Vec<f64> = frame_times_ms
        .iter()
        .copied()
        .filter(|t| t.is_finite() && *t > 0.0)
        .collect();
    if samples.len() < 2 {
        return None;
    }

    let n = samples.len() as f64;
    let sum: f64 = samples.iter().sum();
    let avg_frame_time_ms = sum / n;
    let avg_fps = 1000.0 / avg_frame_time_ms;

    let mean = avg_frame_time_ms;
    let variance = samples
        .iter()
        .map(|t| {
            let d = t - mean;
            d * d
        })
        .sum::<f64>()
        / n;

    // 1% / 0.1% lows: sort frame times ascending (slowest frames last),
    // take the mean of the worst percentile, convert to FPS.
    let mut sorted = samples;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let low_1pct_fps = percentile_low_fps(&sorted, 0.01);
    let low_0_1pct_fps = percentile_low_fps(&sorted, 0.001);

    Some(FrameTimeStats {
        sample_count: sorted.len(),
        avg_fps,
        low_1pct_fps,
        low_0_1pct_fps,
        frame_time_variance_ms2: variance,
        avg_frame_time_ms,
    })
}

fn percentile_low_fps(sorted_asc_ms: &[f64], fraction: f64) -> f64 {
    let n = sorted_asc_ms.len();
    let count = ((n as f64) * fraction).ceil() as usize;
    let count = count.max(1).min(n);
    let worst = &sorted_asc_ms[n - count..];
    let mean_ms = worst.iter().sum::<f64>() / (count as f64);
    1000.0 / mean_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_constant_16ms_frames() {
        // mock: 100 identical 16.0 ms frames (~62.5 FPS)
        let frames = vec![16.0_f64; 100];
        let stats = compute_stats_ms(&frames).expect("stats");
        assert_eq!(stats.sample_count, 100);
        assert!((stats.avg_fps - 62.5).abs() < 1e-9);
        assert!((stats.low_1pct_fps - 62.5).abs() < 1e-9);
        assert!((stats.low_0_1pct_fps - 62.5).abs() < 1e-9);
        assert!(stats.frame_time_variance_ms2.abs() < 1e-12);
    }

    #[test]
    fn mock_spike_lowers_1pct() {
        // mock: mostly 10 ms (~100 FPS), one 50 ms spike
        let mut frames = vec![10.0_f64; 99];
        frames.push(50.0);
        let stats = compute_stats_ms(&frames).expect("stats");
        assert!(stats.avg_fps < 100.0);
        assert!(stats.low_1pct_fps < stats.avg_fps);
        assert!(stats.frame_time_variance_ms2 > 0.0);
    }

    #[test]
    fn rejects_empty_or_non_positive() {
        assert!(compute_stats_ms(&[]).is_none());
        assert!(compute_stats_ms(&[16.0]).is_none());
        assert!(compute_stats_ms(&[0.0, -1.0]).is_none());
    }
}
