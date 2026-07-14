---
title: Configuration
description: Current Rust runtime configuration and intended precedence.
---

# Configuration

Krometrail currently has no user configuration file or browser-control CLI flags. MCP lifecycle tool inputs select managed launch versus explicit local attachment, profiles, initial URLs, executables, and endpoints through their generated schemas.

The root runtime recognizes these environment variables:

| Variable | Behavior |
| --- | --- |
| `KROMETRAIL_DATA_DIR` | Overrides the local recording/index and managed-profile data root. |
| `KROMETRAIL_DISK_BUDGET_BYTES` | Sets the positive global recording budget in decimal bytes. |
| `KROMETRAIL_PROFILE_ROOT` | Overrides the managed-browser profile directory. |

Invalid disk-budget input prevents startup with a structured error. Built-in platform data directories and the default 10 GB budget apply when variables are absent.

The broader configuration precedence remains command-line arguments, environment variables, user configuration, then built-in defaults as defined in [`SPEC.md`](../SPEC.md) and [`ARCHITECTURE.md`](../ARCHITECTURE.md). Capture settings, capability selection, concurrency, logging, and a user configuration file are not yet public configuration surfaces.
