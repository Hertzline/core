//! Network latency / jitter / packet-loss collector (AC-1.4).
//!
//! Measures a user-chosen host via injectable ping samples. Live path shells out
//! to the OS `ping` tool (no extra crate); mocks supply labeled RTT sequences.

use std::process::Command;

use crate::session::{MeasurementSession, MetricF64, NetworkSummary, TextMetric};

/// One echo attempt: `rtt_ms = None` means the probe counted a loss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingSample {
    pub rtt_ms: Option<f64>,
}

/// Mean RTT, RFC 3393-style mean absolute consecutive difference jitter, and loss %.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RttSummary {
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_percent: f64,
    pub received: usize,
    pub sent: usize,
}

/// Summarize RTT samples. Returns `None` when `samples` is empty.
pub fn summarize_rtt_ms(samples: &[PingSample]) -> Option<RttSummary> {
    if samples.is_empty() {
        return None;
    }
    let sent = samples.len();
    let received: Vec<f64> = samples
        .iter()
        .filter_map(|s| s.rtt_ms)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    let received_n = received.len();
    let loss = ((sent - received_n) as f64) * 100.0 / (sent as f64);

    if received_n == 0 {
        return Some(RttSummary {
            latency_ms: f64::NAN,
            jitter_ms: f64::NAN,
            packet_loss_percent: loss,
            received: 0,
            sent,
        });
    }

    let latency = received.iter().sum::<f64>() / (received_n as f64);
    let jitter = if received_n >= 2 {
        let mut abs_diff_sum = 0.0;
        for window in received.windows(2) {
            abs_diff_sum += (window[1] - window[0]).abs();
        }
        abs_diff_sum / ((received_n - 1) as f64)
    } else {
        0.0
    };

    Some(RttSummary {
        latency_ms: latency,
        jitter_ms: jitter,
        packet_loss_percent: loss,
        received: received_n,
        sent,
    })
}

/// Map host + samples to a network summary (empty / all-lost stay explicit).
pub fn network_summary_from_samples(host: &str, samples: &[PingSample]) -> NetworkSummary {
    let host = host.trim();
    if host.is_empty() {
        return NetworkSummary::all_unavailable("network host not set");
    }

    match summarize_rtt_ms(samples) {
        None => NetworkSummary {
            network_host: TextMetric::available(host),
            network_latency_ms: MetricF64::unavailable("no ping samples"),
            network_jitter_ms: MetricF64::unavailable("no ping samples"),
            network_packet_loss_percent: MetricF64::unavailable("no ping samples"),
        },
        Some(s) if s.received == 0 => NetworkSummary {
            network_host: TextMetric::available(host),
            network_latency_ms: MetricF64::unavailable("all probes lost"),
            network_jitter_ms: MetricF64::unavailable("all probes lost"),
            network_packet_loss_percent: MetricF64::available(s.packet_loss_percent, "percent"),
        },
        Some(s) => NetworkSummary {
            network_host: TextMetric::available(host),
            network_latency_ms: MetricF64::available(s.latency_ms, "ms"),
            network_jitter_ms: MetricF64::available(s.jitter_ms, "ms"),
            network_packet_loss_percent: MetricF64::available(s.packet_loss_percent, "percent"),
        },
    }
}

/// Pluggable ping probe (unit tests inject labeled mocks).
pub trait PingProbe {
    fn probe(&mut self, host: &str, count: usize) -> Result<Vec<PingSample>, String>;
}

/// Fixed sample sequence for tests (ignores host).
pub struct StaticPingProbe {
    pub samples: Result<Vec<PingSample>, String>,
}

impl PingProbe for StaticPingProbe {
    fn probe(&mut self, _host: &str, _count: usize) -> Result<Vec<PingSample>, String> {
        self.samples.clone()
    }
}

/// OS `ping` probe (Windows `ping -n`, Unix `ping -c`).
pub struct OsPingProbe;

impl PingProbe for OsPingProbe {
    fn probe(&mut self, host: &str, count: usize) -> Result<Vec<PingSample>, String> {
        let count = count.max(1);
        let output = {
            #[cfg(target_os = "windows")]
            {
                Command::new("ping")
                    .args(["-n", &count.to_string(), host])
                    .output()
            }
            #[cfg(not(target_os = "windows"))]
            {
                Command::new("ping")
                    .args(["-c", &count.to_string(), host])
                    .output()
            }
        }
        .map_err(|e| format!("failed to spawn ping: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let text = if stdout.trim().is_empty() {
            stderr.into_owned()
        } else {
            stdout.into_owned()
        };

        let mut samples = parse_ping_output(&text);
        if samples.is_empty() {
            // ping ran but no parseable RTTs -- treat as count losses if exit failed,
            // else unavailable via Err so the collector can label explicitly.
            if !output.status.success() {
                samples = vec![PingSample { rtt_ms: None }; count];
            } else {
                return Err(format!(
                    "ping produced no parseable RTT lines for host {host}"
                ));
            }
        }
        // If fewer lines than requested, pad losses so loss% stays honest.
        while samples.len() < count {
            samples.push(PingSample { rtt_ms: None });
        }
        Ok(samples)
    }
}

/// Extract RTT milliseconds from common Windows / Unix ping stdout lines.
pub fn parse_ping_output(text: &str) -> Vec<PingSample> {
    let mut out = Vec::new();
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        // Skip summary lines that also contain "time".
        if lower.contains("average")
            || lower.contains("minimum")
            || lower.contains("round-trip")
            || lower.contains("rtt min")
        {
            continue;
        }
        if let Some(ms) = extract_time_ms(line) {
            out.push(PingSample { rtt_ms: Some(ms) });
            continue;
        }
        // Explicit timeout / unreachable lines count as losses when present.
        if lower.contains("timed out")
            || lower.contains("request timed out")
            || lower.contains("100% packet loss")
            || (lower.contains("unreachable") && lower.contains("host"))
        {
            // Only count per-probe timeout lines, not the aggregate summary.
            if lower.contains("request timed out")
                || lower.contains("timed out.")
                || lower.starts_with("request timed out")
            {
                out.push(PingSample { rtt_ms: None });
            }
        }
    }
    out
}

fn extract_time_ms(line: &str) -> Option<f64> {
    // Windows: time=12ms / time<1ms ; Unix: time=12.3 ms
    let lower = line.to_ascii_lowercase();
    let idx = lower.find("time")?;
    let after = lower[idx + 4..].trim_start();
    let after = after
        .strip_prefix('=')
        .or_else(|| after.strip_prefix('<'))
        .unwrap_or(after)
        .trim_start();
    let mut num = String::new();
    for c in after.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            break;
        }
    }
    if num.is_empty() {
        // time<1ms -> treat as 1ms upper bound when '<' was used
        if lower.contains("time<") {
            return Some(1.0);
        }
        return None;
    }
    num.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)
}

/// Network collector for a user-chosen host.
pub struct NetworkCollector<P: PingProbe> {
    probe: P,
    host: String,
    count: usize,
}

impl<P: PingProbe> NetworkCollector<P> {
    pub fn new(probe: P, host: impl Into<String>) -> Self {
        Self {
            probe,
            host: host.into(),
            count: 4,
        }
    }

    pub fn with_count(mut self, count: usize) -> Self {
        self.count = count.max(1);
        self
    }

    pub fn collect(&mut self) -> NetworkSummary {
        let host = self.host.trim();
        if host.is_empty() {
            return NetworkSummary::all_unavailable("network host not set");
        }
        match self.probe.probe(host, self.count) {
            Ok(samples) => network_summary_from_samples(host, &samples),
            Err(reason) => NetworkSummary {
                network_host: TextMetric::available(host),
                network_latency_ms: MetricF64::unavailable(reason.clone()),
                network_jitter_ms: MetricF64::unavailable(reason.clone()),
                network_packet_loss_percent: MetricF64::unavailable(reason),
            },
        }
    }

    pub fn collect_into(&mut self, session: &mut MeasurementSession) -> NetworkSummary {
        let summary = self.collect();
        session.network = summary.clone();
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionMeta;

    #[test]
    fn mock_rtt_summary_latency_jitter_loss() {
        // mock: 3 hits + 1 loss
        let samples = vec![
            PingSample {
                rtt_ms: Some(10.0),
            },
            PingSample {
                rtt_ms: Some(14.0),
            },
            PingSample {
                rtt_ms: Some(12.0),
            },
            PingSample { rtt_ms: None },
        ];
        let s = summarize_rtt_ms(&samples).expect("summary");
        assert_eq!(s.sent, 4);
        assert_eq!(s.received, 3);
        assert!((s.latency_ms - 12.0).abs() < 1e-9);
        // received = [10,14,12]; diffs 4 and 2; mean = 3
        assert!((s.jitter_ms - 3.0).abs() < 1e-9);
        assert!((s.packet_loss_percent - 25.0).abs() < 1e-9);

        let summary = network_summary_from_samples("1.1.1.1", &samples);
        match summary.network_host {
            TextMetric::Available { value } => assert_eq!(value, "1.1.1.1"),
            TextMetric::Unavailable { reason } => panic!("{reason}"),
        }
        match summary.network_latency_ms {
            MetricF64::Available { value, unit } => {
                assert_eq!(unit, "ms");
                assert!((value - 12.0).abs() < 1e-9);
            }
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
    }

    #[test]
    fn mock_all_lost_keeps_loss_available() {
        // mock: total loss
        let samples = vec![PingSample { rtt_ms: None }; 4];
        let summary = network_summary_from_samples("example.test", &samples);
        match summary.network_latency_ms {
            MetricF64::Unavailable { reason } => assert!(reason.contains("lost")),
            MetricF64::Available { .. } => panic!("must not invent latency"),
        }
        match summary.network_packet_loss_percent {
            MetricF64::Available { value, .. } => assert!((value - 100.0).abs() < 1e-9),
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
    }

    #[test]
    fn empty_host_unavailable() {
        let summary = network_summary_from_samples("  ", &[]);
        match summary.network_host {
            TextMetric::Unavailable { .. } => {}
            TextMetric::Available { .. } => panic!("empty host"),
        }
    }

    #[test]
    fn mock_collector_into_session() {
        // mock: StaticPingProbe fixture
        let mut collector = NetworkCollector::new(
            StaticPingProbe {
                samples: Ok(vec![
                    PingSample {
                        rtt_ms: Some(20.0),
                    },
                    PingSample {
                        rtt_ms: Some(22.0),
                    },
                ]),
            },
            "mock.host",
        );
        let mut session = MeasurementSession::new(SessionMeta {
            session_id: "mock-net".into(),
            started_at_unix_ms: 1,
            ended_at_unix_ms: None,
            target_process: None,
            os: "test".into(),
            notes: Some("mock network".into()),
        });
        collector.collect_into(&mut session);
        match session.network.network_latency_ms {
            MetricF64::Available { value, .. } => assert!((value - 21.0).abs() < 1e-9),
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
        match session.network.network_packet_loss_percent {
            MetricF64::Available { value, .. } => assert!((value - 0.0).abs() < 1e-9),
            MetricF64::Unavailable { reason } => panic!("{reason}"),
        }
    }

    #[test]
    fn parse_windows_style_ping_line() {
        let text = "Reply from 1.1.1.1: bytes=32 time=12ms TTL=56\n";
        let samples = parse_ping_output(text);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].rtt_ms, Some(12.0));
    }

    #[test]
    fn parse_unix_style_ping_line() {
        let text = "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.345 ms\n";
        let samples = parse_ping_output(text);
        assert_eq!(samples.len(), 1);
        assert!((samples[0].rtt_ms.unwrap() - 12.345).abs() < 1e-9);
    }

    #[test]
    fn parse_skips_summary_average_line() {
        let text = "\
Reply from 1.1.1.1: bytes=32 time=10ms TTL=56
Minimum = 10ms, Maximum = 10ms, Average = 10ms
";
        let samples = parse_ping_output(text);
        assert_eq!(samples.len(), 1);
    }
}
