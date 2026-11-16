//! AC / battery power source probe (honest: Windows SYSTEM_POWER_STATUS; Linux sysfs).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    /// ACLineStatus online / AC adapter present.
    PluggedIn,
    /// Running on battery.
    OnBattery,
    /// OS did not report a usable status.
    Unknown,
}

impl PowerSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PluggedIn => "plugged_in",
            Self::OnBattery => "on_battery",
            Self::Unknown => "unknown",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::PluggedIn => "Plugged in",
            Self::OnBattery => "On battery",
            Self::Unknown => "Power source unknown",
        }
    }
}

impl fmt::Display for PowerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Read whether the machine is on AC or battery.
pub fn read_power_source() -> PowerSource {
    #[cfg(windows)]
    {
        return read_windows_power_source();
    }
    #[cfg(target_os = "linux")]
    {
        return read_linux_power_source();
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        PowerSource::Unknown
    }
}

#[cfg(windows)]
fn read_windows_power_source() -> PowerSource {
    #[repr(C)]
    struct SystemPowerStatus {
        ac_line_status: u8,
        battery_flag: u8,
        battery_life_percent: u8,
        system_status_flag: u8,
        battery_life_time: u32,
        battery_full_life_time: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
    }

    let mut status = SystemPowerStatus {
        ac_line_status: 255,
        battery_flag: 0,
        battery_life_percent: 0,
        system_status_flag: 0,
        battery_life_time: 0,
        battery_full_life_time: 0,
    };
    // SAFETY: kernel32 GetSystemPowerStatus with a valid SYSTEM_POWER_STATUS pointer.
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok == 0 {
        return PowerSource::Unknown;
    }
    match status.ac_line_status {
        0 => PowerSource::OnBattery,
        1 => PowerSource::PluggedIn,
        _ => PowerSource::Unknown,
    }
}

#[cfg(target_os = "linux")]
fn read_linux_power_source() -> PowerSource {
    use std::fs;
    use std::path::Path;

    let base = Path::new("/sys/class/power_supply");
    let Ok(entries) = fs::read_dir(base) else {
        return PowerSource::Unknown;
    };
    let mut saw_battery = false;
    let mut online: Option<bool> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let typ = fs::read_to_string(path.join("type"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if typ == "mains" || typ == "usb" {
            if let Ok(raw) = fs::read_to_string(path.join("online")) {
                online = Some(raw.trim() == "1");
            }
        } else if typ == "battery" {
            saw_battery = true;
            if let Ok(raw) = fs::read_to_string(path.join("status")) {
                let s = raw.trim().to_ascii_lowercase();
                if s == "charging" || s == "full" {
                    online = Some(true);
                } else if s == "discharging" {
                    online = Some(false);
                }
            }
        }
    }
    match online {
        Some(true) => PowerSource::PluggedIn,
        Some(false) => PowerSource::OnBattery,
        None if saw_battery => PowerSource::OnBattery,
        None => PowerSource::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_stable() {
        assert_eq!(PowerSource::PluggedIn.as_str(), "plugged_in");
        assert_eq!(PowerSource::OnBattery.label(), "On battery");
    }
}
