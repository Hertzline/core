<div align="center">

<a href="https://github.com/Hertzline/core"><img src="https://capsule-render.vercel.app/api?type=waving&height=180&color=0:0f1412,55:12352c,100:0f6b4c&text=core&fontColor=e8eeeb&fontSize=64&fontAlignY=45&animation=twinkling" alt="Hertzline core" width="100%" /></a>

<img src="https://readme-typing-svg.demolab.com?font=IBM+Plex+Mono&weight=500&size=16&color=2A9B7A&center=true&vCenter=true&width=560&height=34&duration=2800&pause=1200&lines=Measurement+%2B+benchmark+%2B+actions;apply+%2F+restore+%2F+verify;Library+only+--+no+UI;No+unverified+FPS+claims" alt="Hertzline core" />

<p>
  <a href="https://github.com/Hertzline"><img src="https://skillicons.dev/icons?i=github&theme=dark" width="44" height="44" alt="GitHub" /></a>&nbsp;
  <a href="https://www.rust-lang.org"><img src="https://skillicons.dev/icons?i=rust&theme=dark" width="44" height="44" alt="Rust" /></a>&nbsp;
  <a href="https://github.com/Hertzline/app"><img src="https://skillicons.dev/icons?i=tauri&theme=dark" width="44" height="44" alt="Used by app" /></a>
</p>

</div>

<br/>

<img src="https://capsule-render.vercel.app/api?type=rect&height=3&color=0:0f1412,35:2a9b7a,70:c9a227,100:0f1412" width="100%" alt="" />

<br/>

<p align="center"><img src="https://readme-typing-svg.demolab.com?font=IBM+Plex+Mono&weight=700&size=18&color=2A9B7A&center=true&vCenter=true&width=420&height=40&duration=1&pause=1000000&letterSpacing=6px&lines=WHAT+THIS+REPO+IS" alt="What this repo is" /></p>

Measurement, benchmark, actions (apply / restore / verify), snapshots, profiles, game detection, and export for [Hertzline](https://github.com/Hertzline).

Library only: no UI, no permanent elevation.

<img src="https://capsule-render.vercel.app/api?type=rect&height=3&color=0:0f1412,35:2a9b7a,70:c9a227,100:0f1412" width="100%" alt="" />

<br/>

<p align="center"><img src="https://readme-typing-svg.demolab.com?font=IBM+Plex+Mono&weight=700&size=18&color=2A9B7A&center=true&vCenter=true&width=420&height=40&duration=1&pause=1000000&letterSpacing=6px&lines=LAYOUT" alt="Layout" /></p>

| Path | Role |
|---|---|
| `crates/hertzline-core` | Core library |
| `spikes/D1.md` | Desktop stack decision |
| `spikes/D3.md` | Frame-time capture decision |
| `spikes/D4.md` | Sensors + license decision |
| `spikes/d1-tauri` / `spikes/d1-avalonia` | HZ-004 spikes |

<img src="https://capsule-render.vercel.app/api?type=rect&height=3&color=0:0f1412,35:2a9b7a,70:c9a227,100:0f1412" width="100%" alt="" />

<br/>

<p align="center"><img src="https://readme-typing-svg.demolab.com?font=IBM+Plex+Mono&weight=700&size=18&color=2A9B7A&center=true&vCenter=true&width=420&height=40&duration=1&pause=1000000&letterSpacing=6px&lines=BUILD" alt="Build" /></p>

```bash
cargo test -p hertzline-core --locked
cargo fmt -p hertzline-core -- --check
cargo clippy -p hertzline-core --locked -- -D warnings
cargo deny check licenses
```

CI: `windows-latest`, `ubuntu-latest`, `macos-latest` (lint + tests + pin/license via `Hertzline/.github`).

### Probes

```bash
cargo run -p hertzline-core --example d3_probe --locked
cargo run -p hertzline-core --example d4_probe --locked
```

### Focused suites

```bash
cargo test -p hertzline-core --locked session::
cargo test -p hertzline-core --locked frame_capture::
cargo test -p hertzline-core --locked collectors::
cargo test -p hertzline-core --locked snapshot::
cargo test -p hertzline-core --locked action::
cargo test -p hertzline-core --locked restore::
```

<p align="center"><sub><i>Clean measurements are like a well-refereed match: when everything works, nobody invents the score.</i></sub></p>

## License

[MIT](LICENSE) · [NOTICE](NOTICE)

<img src="https://capsule-render.vercel.app/api?type=waving&section=footer&height=110&color=0:0f1412,55:12352c,100:0f6b4c" alt="" width="100%" />
