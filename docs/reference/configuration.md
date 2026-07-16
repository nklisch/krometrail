---
title: Configuration reference
description: Current Krometrail data, storage, profile, and capture settings.
---

# Configuration reference

Krometrail currently has no user configuration file or browser-control command-line flags. Your agent selects browser launch or local attachment, profile, initial URL, executable, and endpoint through Krometrail's MCP browser tools.

## Environment variables

| Variable | What it changes |
| --- | --- |
| `KROMETRAIL_DATA_DIR` | Local recording, index, and managed-profile data root. |
| `KROMETRAIL_DISK_BUDGET_BYTES` | Positive global recording budget in decimal bytes. |
| `KROMETRAIL_PROFILE_ROOT` | Managed-browser profile directory. |

When these variables are absent, Krometrail uses the platform data directory and a 10 GB disk budget. An invalid disk-budget value prevents startup and reports the failing setting.

## Capture stride

A browser start or attach request can set `every_nth_frame` from 1 through 60. It defaults to 1.

This is a relative request to Chrome, not an exact frames-per-second setting. The value remains fixed for that browser session; start a new session to use another stride. Krometrail reports the requested stride separately from observed cadence and known capture gaps.

Higher values intentionally sample fewer frames. They do not hide queue drops, storage failures, visibility pauses, or other known losses.
