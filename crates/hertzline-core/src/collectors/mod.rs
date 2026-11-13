//! Hardware, disk, and network collectors (HZ-012 / AC-1.2--AC-1.4).
//!
//! Missing sensors stay unavailable with an explicit reason -- never invented.
//! Unit tests inject labeled mocks; live paths use sysinfo (D4) and OS ping.

mod disk;
mod hardware;
mod network;

pub use disk::{
    disk_summary_from_delta, DiskByteCounters, DiskCollector, DiskIoSource, SysinfoDiskIo,
};
pub use hardware::{
    hardware_summary_from_sample, HardwareCollector, HardwareSample, HardwareSampler,
    SysinfoHardwareSampler,
};
pub use network::{
    network_summary_from_samples, parse_ping_output, summarize_rtt_ms, NetworkCollector,
    OsPingProbe, PingProbe, PingSample, StaticPingProbe,
};
