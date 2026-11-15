# Security Policy

Report vulnerabilities privately as described in the organization
[SECURITY.md](https://github.com/Hertzline/.github/blob/main/SECURITY.md).

Do not open public issues for undisclosed vulnerabilities.

## Core surface

This repository owns measurement, benchmarks, actions, snapshots, profiles, and
export. It must not inject into games, scrape undocumented APIs, or ship
product telemetry by default.

HZ-055 threat model and helper audit live in the sibling
[`helper` docs](https://github.com/Hertzline/helper/blob/main/docs/THREAT_MODEL.md).

Rules:

- Overlay methods are non-injection only (`injects_into_game` is always false).
- Snapshots are written before mutating actions (AC-7.1).
- Missing sensors and unsupported OS paths are labeled unavailable / not possible.
