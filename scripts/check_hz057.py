#!/usr/bin/env python3
"""HZ-057 local check: integration suite + per-OS checklist gates for Hertzline/core."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEST = ROOT / "crates" / "hertzline-core" / "tests" / "hz057_integration.rs"
CHECKLIST = ROOT / "docs" / "HZ057_CHECKLIST.md"
CI = ROOT / ".github" / "workflows" / "ci.yml"

REQUIRED_TEST_MARKERS = [
    "ac1_measure_smoke",
    "ac2_benchmark_reproducibility",
    "ac3_game_fixtures",
    "ac4_action_explanations",
    "ac5_everyday_smoke",
    "ac6_overlay_anti_cheat",
    "ac7_crash_rollback",
    "ac8_explanation_local_measurement",
    "LabeledMockWindows",
    "recover_from_crash",
]

REQUIRED_CHECKLIST = [
    "HZ-057",
    "windows-latest",
    "ubuntu-latest",
    "macos-latest",
    "AC-1",
    "AC-2",
    "AC-7",
    "crash-rollback",
    "benchmark reproducibility",
    "Sign-off",
]


def main() -> int:
    if not TEST.is_file():
        print("FAIL missing", TEST.relative_to(ROOT))
        return 1
    if not CHECKLIST.is_file():
        print("FAIL missing", CHECKLIST.relative_to(ROOT))
        return 1
    if not CI.is_file():
        print("FAIL missing", CI.relative_to(ROOT))
        return 1

    test_text = TEST.read_text(encoding="utf-8")
    missing = [m for m in REQUIRED_TEST_MARKERS if m not in test_text]
    if missing:
        print("FAIL hz057_integration.rs missing:", ", ".join(missing))
        return 1

    checklist = CHECKLIST.read_text(encoding="utf-8")
    missing_c = [m for m in REQUIRED_CHECKLIST if m not in checklist]
    if missing_c:
        print("FAIL HZ057_CHECKLIST.md missing:", ", ".join(missing_c))
        return 1

    ci = CI.read_text(encoding="utf-8")
    for marker in ("check_hz057.py", "hz057_integration"):
        if marker not in ci:
            print("FAIL ci.yml missing", marker)
            return 1

    print("check_hz057: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
