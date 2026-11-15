#!/usr/bin/env python3
"""HZ-056 local check: CI pin/license/fork-secret hardening for Hertzline/core."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CI = ROOT / ".github" / "workflows" / "ci.yml"
DENY = ROOT / "deny.toml"
META_SHA = "f4f947f31a5da82d0ca4828917490d2a1d1217b0"
SHA40 = re.compile(r"@[0-9a-fA-F]{40}\b")


def main() -> int:
    if not CI.is_file():
        print("FAIL missing", CI.relative_to(ROOT))
        return 1
    if not DENY.is_file():
        print("FAIL missing deny.toml")
        return 1

    text = CI.read_text(encoding="utf-8")
    required = [
        "permissions:",
        "contents: read",
        "has-secrets: false",
        f"reusable-ci.yml@{META_SHA}",
        f"meta-ref: {META_SHA}",
        "pin-check: true",
        "license-audit: true",
        "needs: [shared]",
    ]
    absent = [m for m in required if m not in text]
    if absent:
        print("FAIL ci.yml missing:", ", ".join(absent))
        return 1

    if "secrets: inherit" in text or re.search(r"(?m)^\s*secrets:\s*$", text):
        print("FAIL ci.yml must not pass secrets to jobs")
        return 1

    for m in re.finditer(r"(?m)^\s*uses:\s*(\S+)", text):
        value = m.group(1).strip().strip("\"'")
        if value.startswith("./") or value.startswith("$/"):
            continue
        if not SHA40.search(value):
            print("FAIL unpinned uses:", value)
            return 1

    print("check_hz056: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
