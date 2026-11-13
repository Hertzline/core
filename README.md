# Hertzline / core

Measurement, benchmark, actions (apply / restore / verify), snapshots, profiles, game detection, and export for [Hertzline](https://github.com/Hertzline).

Library only: no UI, no permanent elevation.

## Layout

| Path | Role |
|---|---|
| `crates/hertzline-core` | Core library |
| `spikes/d1-tauri` | HZ-004 Option A: Rust + Tauri 2 window + IPC |
| `spikes/d1-avalonia` | HZ-004 Option B: .NET + Avalonia window + IPC stub |
| `spikes/D1.md` | Desktop stack decision evidence |
| `spikes/D3.md` | Frame-time capture decision evidence |
| `spikes/d3-tools/` | Optional PresentMon download for local probe (gitignored binaries) |
| `spikes/D4.md` | Sensor sources + license decision evidence |
| `spikes/d4-licenses/` | Verified license texts (sysinfo MIT; LHM MPL rejected; nvml-wrapper) |

## Build

```bash
cargo test -p hertzline-core --locked
cargo fmt -p hertzline-core -- --check
cargo clippy -p hertzline-core --locked -- -D warnings
cargo deny check licenses
```

CI (`ci` workflow): `windows-latest`, `ubuntu-latest`, `macos-latest` for lint +
`hertzline-core` tests; shared job runs pin-check + license audit via
`Hertzline/.github` reusable workflow (SHA pinned in `.github/workflows/ci.yml`).

### D1 Tauri spike

Requires a working [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) install on the host OS.

```bash
cd spikes/d1-tauri
npm ci
npx tauri build --no-bundle
```

Smoke (opens a window briefly, runs IPC, exits):

```bash
# Windows PowerShell
$env:HERTZLINE_D1_SMOKE=1
# path may be workspace target/release/d1-tauri.exe after build
```

### D1 Avalonia spike

Requires .NET SDK 8+.

```bash
cd spikes/d1-avalonia
dotnet build -c Release
$env:HERTZLINE_D1_SMOKE=1   # PowerShell
dotnet run -c Release --no-build
```

### D3 frame-capture probe

```bash
cargo test -p hertzline-core --locked
cargo run -p hertzline-core --example d3_probe --locked
```

On Windows, set `HERTZLINE_PRESENTMON` to a PresentMon 2.6.0 console binary for an `available` probe (see `spikes/d3-tools/README.md`). Real-time ETW needs admin or the Performance Log Users group.

### D4 sensors probe

```bash
cargo test -p hertzline-core --locked
cargo run -p hertzline-core --example d4_probe --locked
```

Primary stack is `sysinfo` 0.40.3 (MIT). Missing sensors are labeled unavailable. See `spikes/D4.md`.

### Session export (HZ-010 / AC-1.8)

`MeasurementSession` exports CSV and JSON from one ordered field list (identical names and units). Round-trip tests live in `session::export`.

```bash
cargo test -p hertzline-core --locked session::
```

### Frame-time pipeline (HZ-011 / AC-1.1, AC-1.5--AC-1.7)

`FramePipeline::open()` probes the host source, ingests PresentMon CSV (`MsBetweenPresents`) or MangoHud logs (`frametime`), then `finish()` / `finish_into(session)` fills avg FPS, 1% / 0.1% lows, and frame-time variance. Probe failure or macOS not-possible yields `Unavailable` (never invented FPS).

```bash
cargo test -p hertzline-core --locked frame_capture::
```

### Collectors (HZ-012 / AC-1.2--AC-1.4)

`collectors` fills `HardwareSummary`, `DiskSummary`, and `NetworkSummary` on a session:

- Hardware: `sysinfo` CPU/RAM/GPU/VRAM/temps when exposed; GPU clocks and throttle stay unavailable (D4).
- Disk: timed `Disk::usage` read/write throughput; queue depth unavailable until an OS path is wired.
- Network: user-chosen host via injectable ping samples (OS `ping` or mocks); latency, jitter, packet loss.

Missing sensors are labeled unavailable. Unit tests use labeled mocks.

```bash
cargo test -p hertzline-core --locked collectors::
```

## License

[MIT](LICENSE)

Third-party notices: [NOTICE](NOTICE)
