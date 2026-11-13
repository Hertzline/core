# D1 Tauri spike

Minimal Tauri 2 window + IPC into `hertzline-core`.

```bash
npm ci
npx tauri build --no-bundle
```

Smoke:

```bash
# from repo root after build
$env:HERTZLINE_D1_SMOKE=1
.\target\release\d1-tauri.exe
```

See `../D1.md` for measurements and decision.
