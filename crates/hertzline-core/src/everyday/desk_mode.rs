//! Built-in desk modes (Study / Programming / Performance).
//!
//! Maps to OS power presets only. Hertzline does not control fan curves;
//! "quiet" vs "audible fans" is an expected effect of the chosen plan.

use super::power_source::PowerSource;
use super::preset::EverydayPreset;

pub const MODE_STUDY: &str = "study";
pub const MODE_PROGRAMMING: &str = "programming";
pub const MODE_PERFORMANCE: &str = "performance";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeskMode {
    Study,
    Programming,
    Performance,
}

impl DeskMode {
    pub const ALL: [DeskMode; 3] = [Self::Study, Self::Programming, Self::Performance];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Study => MODE_STUDY,
            Self::Programming => MODE_PROGRAMMING,
            Self::Performance => MODE_PERFORMANCE,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Study => "Study",
            Self::Programming => "Programming",
            Self::Performance => "Performance",
        }
    }

    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Study => {
                "Maximum power saving. Fine for reading and light work; keeps the machine quiet."
            }
            Self::Programming => {
                "Pushes for responsive builds and editors, but stays off the max plan so fans stay quieter."
            }
            Self::Performance => {
                "Full high-performance plan. Fans may be audible. Best when plugged in for games or heavy work."
            }
        }
    }

    pub const fn fans_note(self) -> &'static str {
        match self {
            Self::Study => "Expect the quietest fan behavior from the OS power-saver plan.",
            Self::Programming => {
                "Balanced when plugged in — not max fans. Hertzline does not set fan curves."
            }
            Self::Performance => {
                "High-performance plan — fans may spin up and be audible. No direct fan control."
            }
        }
    }

    pub const fn adapts_to_power(self) -> bool {
        matches!(self, Self::Programming | Self::Performance)
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            MODE_STUDY | "studio" => Some(Self::Study),
            MODE_PROGRAMMING | "coding" | "dev" => Some(Self::Programming),
            MODE_PERFORMANCE | "perf" => Some(Self::Performance),
            _ => None,
        }
    }

    /// Resolve the OS power preset for the current AC/battery state.
    pub fn resolve(self, source: PowerSource) -> EverydayPreset {
        match self {
            Self::Study => EverydayPreset::BatterySaver,
            Self::Programming => match source {
                PowerSource::OnBattery => EverydayPreset::BatterySaver,
                PowerSource::PluggedIn | PowerSource::Unknown => EverydayPreset::Balanced,
            },
            Self::Performance => match source {
                PowerSource::OnBattery => EverydayPreset::Balanced,
                PowerSource::PluggedIn | PowerSource::Unknown => EverydayPreset::Performance,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn study_always_saves() {
        assert_eq!(
            DeskMode::Study.resolve(PowerSource::PluggedIn),
            EverydayPreset::BatterySaver
        );
        assert_eq!(
            DeskMode::Study.resolve(PowerSource::OnBattery),
            EverydayPreset::BatterySaver
        );
    }

    #[test]
    fn programming_adapts() {
        assert_eq!(
            DeskMode::Programming.resolve(PowerSource::PluggedIn),
            EverydayPreset::Balanced
        );
        assert_eq!(
            DeskMode::Programming.resolve(PowerSource::OnBattery),
            EverydayPreset::BatterySaver
        );
    }

    #[test]
    fn performance_adapts_on_battery() {
        assert_eq!(
            DeskMode::Performance.resolve(PowerSource::PluggedIn),
            EverydayPreset::Performance
        );
        assert_eq!(
            DeskMode::Performance.resolve(PowerSource::OnBattery),
            EverydayPreset::Balanced
        );
    }
}
