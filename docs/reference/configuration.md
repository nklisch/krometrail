---
title: Configuration
description: Current Rust runtime configuration and intended precedence.
---

# Configuration

Krometrail currently has no user configuration file or browser-control CLI flags. MCP lifecycle tool inputs select managed launch versus explicit local attachment, profiles, initial URLs, executables, and endpoints through their generated schemas.

Each `start_browser` and `attach_browser` MCP request also accepts the generated `every_nth_frame` field. It is an integer from 1 through 60, defaults to 1, is forwarded to CDP screencast capture, and remains immutable for that browser connection. A different stride requires a new browser session; it is reported in lifecycle and capture status.

The root runtime recognizes these environment variables:

| Variable | Behavior |
| --- | --- |
| `KROMETRAIL_DATA_DIR` | Overrides the local recording/index and managed-profile data root. |
| `KROMETRAIL_DISK_BUDGET_BYTES` | Sets the positive global recording budget in decimal bytes. |
| `KROMETRAIL_PROFILE_ROOT` | Overrides the managed-browser profile directory. |

Invalid disk-budget input prevents startup with a structured error. Built-in platform data directories and the default 10 GB budget apply when variables are absent.

The broader intended precedence remains command-line arguments, environment variables, user configuration, then built-in defaults as defined in [`SPEC.md`](../SPEC.md) and [`ARCHITECTURE.md`](../ARCHITECTURE.md). The current executable has no browser-control CLI flags, user configuration file, external capability-selection input, or public inputs for capture format, quality, dimensions, concurrency, or logging; the MCP lifecycle request is the public capture-stride boundary.
