---
title: "FAQ"
description: "Frequently asked questions about Krometrail — installation, configuration, language support, and comparison with alternatives."
---

# Frequently Asked Questions

## What is Krometrail?

Krometrail is an open-source MCP server and CLI that gives AI coding agents runtime debugging and browser observation capabilities. It connects to debuggers in 10 programming languages via the Debug Adapter Protocol (DAP) and records Chrome browser sessions via the Chrome DevTools Protocol (CDP). Agents interact through MCP tools or CLI commands, receiving compressed viewport output designed to fit within LLM context windows at approximately 300–400 tokens per debugging stop.

## What languages does Krometrail support?

Krometrail supports runtime debugging in 10 languages: Python (via debugpy), Node.js and TypeScript (via js-debug), Go (via Delve), Rust (via CodeLLDB), Java (via java-debug), C and C++ (via cppdbg/GDB/LLDB), Ruby (via rdbg), C# (via netcoredbg), Swift (via lldb-dap), and Kotlin (via kotlin-debug-adapter). Each language uses a dedicated adapter that handles debugger lifecycle, breakpoint management, and output formatting.

## How do I configure Krometrail with Claude Code?

Add Krometrail to your Claude Code MCP configuration by adding a `krometrail` entry to your `.mcp.json` file. The entry should specify `"command": "npx"` with `"args": ["-y", "krometrail@latest", "--mcp"]`. Once configured, Claude Code can use Krometrail's MCP tools to launch debug sessions, set breakpoints, step through code, and inspect variables. See the [MCP Configuration guide](/guide/mcp-configuration) for complete setup instructions.

## How does Krometrail compare to other MCP debugging tools?

Krometrail differs from alternatives like AIDB, mcp-debugger, and mcp-dap-server in several key areas. Unlike AIDB, Krometrail uses the standard Debug Adapter Protocol rather than a custom protocol, supporting 10 languages instead of only Python. Compared to mcp-debugger and mcp-dap-server, Krometrail adds browser observation, viewport-aware output compression, and a CLI interface. See the [detailed comparison](/guide/getting-started#how-krometrail-compares) for a full feature matrix.

## What does browser observation capture?

Krometrail's browser observation records six categories of events from Chrome sessions: network requests and responses (with headers, status codes, and timing), console output and errors, DOM mutations, storage changes (localStorage, sessionStorage, cookies), framework state (React component trees, Vue/Pinia stores), and timestamped screenshots. Agents can search, inspect, diff, and replay recorded events using investigation tools.

## How much context window does Krometrail use?

Krometrail's viewport output is designed to be token-efficient. Each debugging stop produces approximately 300–400 tokens, including source context, variable values, call stack, and breakpoint status. The viewport uses progressive compression — as context accumulates, older stops are summarized to stay within budget. Browser observation lenses similarly compress recorded events, with configurable limits on event counts and detail levels.

## Is Krometrail free?

Yes. Krometrail is open-source software released under the MIT License. There are no paid tiers, usage limits, or telemetry. The software runs entirely on your local machine. You can install it via npm (`npx krometrail`) or download a standalone binary from the [CLI installation page](/guide/cli-installation).
