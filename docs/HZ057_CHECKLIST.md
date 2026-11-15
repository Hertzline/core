# HZ-057 -- Per-OS integration checklist

AC-1--AC-8 smoke, crash-rollback, and benchmark reproducibility on real OS
machines/VMs (local host + GitHub Actions `windows-latest` / `ubuntu-latest` /
`macos-latest`).

## How sign-off works

1. Run `cargo test -p hertzline-core --test hz057_integration --locked` (and the
   matching helper/app suites).
2. CI runs the same tests on all three OS runners.
3. A row is signed off when the automated suite exits 0 on that OS. Record the
   machine, OS build, toolchain versions, and CI run id in `PROGRESS.md`.
4. Unavailable interactive elevation (UAC/polkit) is labeled `unavailable`, not
   invented as pass.

## Checklist

| OS | Runner | AC-1 measure | AC-2 bench repro | AC-3 profiles | AC-4 actions | AC-5 everyday | AC-6 overlay | AC-7 crash/restore | AC-8 explanations | Sign-off |
|---|---|---|---|---|---|---|---|---|---|---|
| Windows | local host | automated | automated | automated | automated (live power plan when present) | automated | automated | automated | automated | CI + local |
| Windows | `windows-latest` | automated | automated | automated | automated | automated | automated | automated | automated | CI |
| Linux | `ubuntu-latest` | automated | automated | automated | explanations + capability (live apply OS-gated) | automated | automated | automated | automated | CI |
| macOS | `macos-latest` | automated | automated | automated | honesty matrix + explanations | automated | automated | automated | automated | CI |

## Required gates

- `scripts/check_hz057.py` PASS in `core`, `helper`, and `app`
- `cargo test --test hz057_integration --locked` PASS
- Crash-rollback (`ac7_*`) PASS
- Benchmark reproducibility (`ac2_*`) PASS

## Notes

- Labeled mocks are marked in test names/comments; live collectors/probes use
  the host OS.
- Destructive cleanup / elevated HAGS / System Restore create are not force-run
  in this smoke (honest unavailable / opt-in elsewhere).
