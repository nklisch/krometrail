---
title: Configuration
description: Current configuration status and the intended Rust runtime contracts.
---

# Configuration

The current executable has no user configuration file, environment-variable configuration, browser launch options, or MCP configuration surface. The only available command is `doctor`, which reports that browser transport is not yet available.

Do not copy configuration examples from the historical runtime. When configuration is implemented, startup validation and precedence will follow the contracts in [`SPEC.md`](../SPEC.md) and [`ARCHITECTURE.md`](../ARCHITECTURE.md): command-line arguments, environment variables, user configuration, then built-in defaults.

The intended configuration areas include the browser endpoint or launch profile, data directory and disk budget, capture settings, enabled capabilities, concurrency, and logging. They are not current options and should not be passed to the binary today.
