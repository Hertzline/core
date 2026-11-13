# D3 tools (local only)

Download Intel PresentMon console for Windows probe smoke. Binaries and CSV are gitignored.

```powershell
Invoke-WebRequest `
  -Uri "https://github.com/GameTechDev/PresentMon/releases/download/v2.6.0/PresentMon-2.6.0-x64.exe" `
  -OutFile "PresentMon-2.6.0-x64.exe"
$env:HERTZLINE_PRESENTMON = (Resolve-Path .\PresentMon-2.6.0-x64.exe).Path
```

Verified release: PresentMon **v2.6.0** (2026-09-21), MIT, asset `PresentMon-2.6.0-x64.exe`.
