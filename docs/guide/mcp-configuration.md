---
title: MCP Configuration
description: Current MCP status and the intended Krometrail boundary.
---

# MCP configuration

The current Rust executable does not expose an MCP server. Do not add a Krometrail MCP entry to an agent configuration yet, and do not use historical command examples copied from an earlier runtime.

MCP is an intended boundary for the browser-control and temporal-recording capabilities described in [`SPEC.md`](../SPEC.md) and [`ARCHITECTURE.md`](../ARCHITECTURE.md). Those documents are contracts for work in progress, not proof that a command is available today.

The browser boundary includes Chrome-compatible pages and explicitly debug-enabled Electron renderer processes. Electron's Node main process is outside that boundary. The first real transport implementation will define the supported connection and MCP setup details; this page will be updated when that surface exists.
