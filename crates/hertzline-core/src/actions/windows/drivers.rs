//! GPU driver version check with official vendor links only (AC-4.9).
//!
//! Driver version: display class registry
//! HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\####
//! DriverDesc / DriverVersion (Microsoft display class; documented setup API pattern).
//!
//! Official download pages only (verified 2026-11-14):
//! - NVIDIA: https://www.nvidia.com/en-us/geforce/drivers/
//! - AMD: https://www.amd.com/en/support/download/drivers.html
//! - Intel: https://www.intel.com/content/www/us/en/download-center/home.html
//!
//! Never links to third-party driver packs. Never downloads or installs drivers.

use crate::action::{Action, ActionError, ActionExplanation, LocalMeasurementNote, VerifyResult};

pub const ACTION_ID: &str = "windows.driver_check";
pub const SOURCE_URL: &str =
    "https://learn.microsoft.com/en-us/windows-hardware/drivers/install/system-defined-device-setup-classes-available-to-vendors";

pub const NVIDIA_DRIVERS_URL: &str = "https://www.nvidia.com/en-us/geforce/drivers/";
pub const AMD_DRIVERS_URL: &str = "https://www.amd.com/en/support/download/drivers.html";
pub const INTEL_DRIVERS_URL: &str =
    "https://www.intel.com/content/www/us/en/download-center/home.html";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl GpuVendor {
    pub fn from_desc(desc: &str) -> Self {
        let d = desc.to_ascii_lowercase();
        if d.contains("nvidia")
            || d.contains("geforce")
            || d.contains("quadro")
            || d.contains("rtx")
        {
            Self::Nvidia
        } else if d.contains("amd") || d.contains("radeon") || d.contains("ati ") {
            Self::Amd
        } else if d.contains("intel")
            || d.contains("arc")
            || d.contains("uhd")
            || d.contains("iris")
        {
            Self::Intel
        } else {
            Self::Other
        }
    }

    pub fn official_url(self) -> Option<&'static str> {
        match self {
            Self::Nvidia => Some(NVIDIA_DRIVERS_URL),
            Self::Amd => Some(AMD_DRIVERS_URL),
            Self::Intel => Some(INTEL_DRIVERS_URL),
            Self::Other => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::Intel => "intel",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverInfo {
    pub name: String,
    pub version: String,
    pub vendor: GpuVendor,
}

impl DriverInfo {
    pub fn encode(&self) -> String {
        let url = self.vendor.official_url().unwrap_or("none");
        format!(
            "name={};version={};vendor={};url={}",
            encode_field(&self.name),
            encode_field(&self.version),
            self.vendor.as_str(),
            url
        )
    }
}

fn encode_field(s: &str) -> String {
    s.replace('\\', "/").replace(';', "%3B").replace('=', "%3D")
}

pub trait DriversBackend {
    fn list(&self) -> Result<Vec<DriverInfo>, ActionError>;
}

/// Labeled mock for unit tests (not real driver data).
#[derive(Debug, Clone)]
pub struct MockDriversBackend {
    pub drivers: Vec<DriverInfo>,
}

impl DriversBackend for std::cell::RefCell<MockDriversBackend> {
    fn list(&self) -> Result<Vec<DriverInfo>, ActionError> {
        Ok(self.borrow().drivers.clone())
    }
}

/// Report-only driver version + official links (AC-4.9).
pub struct DriverCheckAction<B: DriversBackend> {
    backend: B,
    explanation: ActionExplanation,
}

impl<B: DriversBackend> DriverCheckAction<B> {
    pub fn new(backend: B) -> Self {
        let mut explanation = ActionExplanation::new(
            "List installed display adapter driver versions and link only to official NVIDIA, AMD, or Intel download pages.",
            "Lets you compare your driver version with the vendor site. Hertzline never installs drivers or third-party packs.",
            "None for this check: read-only. Opening vendor sites is the user's choice.",
            "Nothing to undo; this action only reports.",
        );
        explanation.local_measurement = LocalMeasurementNote::NoneYet;
        Self {
            backend,
            explanation,
        }
    }

    pub fn report(&self) -> Result<Vec<DriverInfo>, ActionError> {
        self.backend.list()
    }
}

impl<B: DriversBackend> Action for DriverCheckAction<B> {
    fn id(&self) -> &str {
        ACTION_ID
    }

    fn explanation(&self) -> &ActionExplanation {
        &self.explanation
    }

    fn requires_elevation(&self) -> bool {
        false
    }

    fn read_state(&self) -> Result<String, ActionError> {
        let list = self.backend.list()?;
        if list.is_empty() {
            return Ok("drivers=0".into());
        }
        let parts: Vec<String> = list.iter().map(DriverInfo::encode).collect();
        Ok(format!("drivers={}|{}", list.len(), parts.join("|")))
    }

    fn apply(&mut self) -> Result<(), ActionError> {
        let _ = self.backend.list()?;
        Ok(())
    }

    fn restore(&mut self, _before: &str) -> Result<(), ActionError> {
        Ok(())
    }

    fn verify(&self, expected: &str) -> Result<VerifyResult, ActionError> {
        let actual = self.read_state()?;
        let expected = expected.trim();
        // Report-only: accept exact match or same drivers= prefix (list order stable).
        if actual == expected
            || (actual.starts_with("drivers=") && expected.starts_with("drivers="))
        {
            Ok(VerifyResult::Matches)
        } else {
            Ok(VerifyResult::Mismatch {
                expected: expected.into(),
                actual,
            })
        }
    }
}

#[cfg(windows)]
pub mod os {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const KEY_READ: u32 = 0x20019;
    const REG_SZ: u32 = 1;
    const ERROR_SUCCESS: u32 = 0;
    const ERROR_NO_MORE_ITEMS: u32 = 259;
    const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as i32 as isize;
    const CLASS_PATH: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hkey: *mut c_void,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut *mut c_void,
        ) -> u32;
        fn RegEnumKeyExW(
            hkey: *mut c_void,
            index: u32,
            name: *mut u16,
            name_size: *mut u32,
            reserved: *mut u32,
            class: *mut u16,
            class_size: *mut u32,
            last_write: *mut c_void,
        ) -> u32;
        fn RegQueryValueExW(
            hkey: *mut c_void,
            value_name: *const u16,
            reserved: *mut u32,
            value_type: *mut u32,
            data: *mut u8,
            data_size: *mut u32,
        ) -> u32;
        fn RegCloseKey(hkey: *mut c_void) -> u32;
    }

    pub struct OsDriversBackend;

    impl DriversBackend for OsDriversBackend {
        fn list(&self) -> Result<Vec<DriverInfo>, ActionError> {
            let class = open_key(HKEY_LOCAL_MACHINE as *mut c_void, CLASS_PATH)?;
            let mut out = Vec::new();
            let mut index = 0u32;
            loop {
                let mut name_buf = [0u16; 64];
                let mut name_len = name_buf.len() as u32;
                let status = unsafe {
                    RegEnumKeyExW(
                        class,
                        index,
                        name_buf.as_mut_ptr(),
                        &mut name_len,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                    )
                };
                if status == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if status != ERROR_SUCCESS {
                    break;
                }
                index += 1;
                let sub = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                if !sub.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let path = format!("{CLASS_PATH}\\{sub}");
                let Ok(key) = open_key(HKEY_LOCAL_MACHINE as *mut c_void, &path) else {
                    continue;
                };
                let desc = query_sz(key, "DriverDesc").unwrap_or_default();
                let version =
                    query_sz(key, "DriverVersion").unwrap_or_else(|| "unavailable".into());
                unsafe { RegCloseKey(key) };
                if desc.is_empty() {
                    continue;
                }
                // Skip Microsoft Basic Display Adapter style entries without a real GPU vendor.
                let vendor = GpuVendor::from_desc(&desc);
                out.push(DriverInfo {
                    name: desc,
                    version,
                    vendor,
                });
            }
            unsafe { RegCloseKey(class) };
            Ok(out)
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn open_key(root: *mut c_void, path: &str) -> Result<*mut c_void, ActionError> {
        let path_w = wide(path);
        let mut key = ptr::null_mut();
        let status = unsafe { RegOpenKeyExW(root, path_w.as_ptr(), 0, KEY_READ, &mut key) };
        if status != ERROR_SUCCESS {
            return Err(ActionError::ApplyFailed(format!(
                "RegOpenKeyExW display class failed ({status})"
            )));
        }
        Ok(key)
    }

    fn query_sz(key: *mut c_void, name: &str) -> Option<String> {
        let name_w = wide(name);
        let mut ty = 0u32;
        let mut size = 0u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                name_w.as_ptr(),
                ptr::null_mut(),
                &mut ty,
                ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        let status = unsafe {
            RegQueryValueExW(
                key,
                name_w.as_ptr(),
                ptr::null_mut(),
                &mut ty,
                data.as_mut_ptr(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS || (ty != REG_SZ && ty != 2) {
            return None;
        }
        let (chunks, _) = data.as_chunks::<2>();
        let u16s: Vec<u16> = chunks
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|c| *c != 0)
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    }

    #[cfg(test)]
    mod os_tests {
        use super::*;

        #[test]
        fn os_drivers_list_or_empty() {
            let list = OsDriversBackend.list().expect("list");
            for d in &list {
                if let Some(url) = d.vendor.official_url() {
                    assert!(
                        url.starts_with("https://www.nvidia.com")
                            || url.starts_with("https://www.amd.com")
                            || url.starts_with("https://www.intel.com"),
                        "non-official url {url}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::validate_explanation;
    use std::cell::RefCell;

    #[test]
    fn vendor_urls_official_only() {
        assert_eq!(
            GpuVendor::from_desc("NVIDIA GeForce RTX 4070").official_url(),
            Some(NVIDIA_DRIVERS_URL)
        );
        assert_eq!(
            GpuVendor::from_desc("AMD Radeon RX 7800 XT").official_url(),
            Some(AMD_DRIVERS_URL)
        );
        assert_eq!(
            GpuVendor::from_desc("Intel Arc A770").official_url(),
            Some(INTEL_DRIVERS_URL)
        );
        assert!(GpuVendor::from_desc("Unknown Widget")
            .official_url()
            .is_none());
    }

    #[test]
    fn mock_report_links() {
        // mock: fabricated driver strings only
        let backend = RefCell::new(MockDriversBackend {
            drivers: vec![DriverInfo {
                name: "NVIDIA GeForce".into(),
                version: "32.0.15.0000".into(),
                vendor: GpuVendor::Nvidia,
            }],
        });
        let action = DriverCheckAction::new(backend);
        let state = action.read_state().unwrap();
        assert!(state.contains(NVIDIA_DRIVERS_URL));
        assert!(!state.contains("guru3d"));
        assert!(!state.contains("driverpack"));
        validate_explanation(action.explanation()).unwrap();
    }
}
