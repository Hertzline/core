# D4 license texts (HZ-007)

Downloaded from official sources on 2026-11-13. Used as on-disk evidence for SPDX decisions; not linked into the crate.

| File | Project | SPDX / note | Source |
|---|---|---|---|
| `sysinfo-LICENSE-MIT.txt` | sysinfo | MIT | https://raw.githubusercontent.com/GuillaumeGomez/sysinfo/main/LICENSE |
| `LibreHardwareMonitor-LICENSE-MPL-2.0.txt` | LibreHardwareMonitor | MPL-2.0 (**rejected** for linking) | https://raw.githubusercontent.com/LibreHardwareMonitor/LibreHardwareMonitor/master/LICENSE |
| `nvml-wrapper-LICENSE-MIT.txt` | nvml-wrapper | MIT (crate only) | https://raw.githubusercontent.com/rust-nvml/nvml-wrapper/main/LICENSE-MIT |
| `nvml-wrapper-LICENSE-APACHE.txt` | nvml-wrapper | Apache-2.0 (crate only) | https://raw.githubusercontent.com/rust-nvml/nvml-wrapper/main/LICENSE-APACHE |

NVIDIA NVML **runtime** is covered by the NVIDIA driver / SDK EULA, not MIT. Hertzline may load a system-installed NVML later; it must not redistribute the NVIDIA SDK.
