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

## Build

```bash
cargo test -p hertzline-core
```

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

## License

[MIT](LICENSE)

Third-party notices: [NOTICE](NOTICE)
