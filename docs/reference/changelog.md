---
title: Changelog
description: Release history for Krometrail.
---

# Changelog

## v0.2.9

### Features

- **`doctor --fix`** — new flag prints a copy-paste shell script to install all missing debuggers
- **`EXIT_PREREQUISITES` (exit code 6)** — adapter prerequisite failures now return a distinct exit code, separate from resource-not-found (3); agents can distinguish "debugger missing" from "session not found"
- **`fixCommand` in error envelope** — JSON error output for prerequisite failures now includes a `fixCommand` field with the exact install command

### Fixes

- `AdapterNotFoundError` message no longer references the non-existent `debug_status` command — now says `krometrail doctor`
- `doctor --json` now reports correct version for Go/Delve when `dlv` is in `~/go/bin` but not on `$PATH`
- `LaunchError` now carries a `cause_type` field (`spawn_failed`, `connection_timeout`, `early_exit`) enabling targeted error messages
- Error messages in text mode now include actionable guidance: install hints, PATH advice, timeout context

### Internal

- All 10 adapters now return `fixCommand` from `checkPrerequisites()` — single source of truth instead of hardcoded map in doctor
- New e2e tests: `doctor-completeness`, `prerequisite-errors` (containerized), `install-flow`, `error-exit-codes`

## v0.2.8

### Fixes

- MCP install docs now default to binary (one-liner install) with npx/bunx as tabbed alternatives
- Fixed `--tools browser` → `--tools=browser` syntax in all docs and one-liners (prevents citty misparse)
- Fixed `--help` test to check for `chrome` subcommand after CLI rename

### Internal

- Added regression tests for `--tools=X` syntax and one-liner flag validation

## v0.2.7

### Features

- **`chrome run-steps` / `chrome_run_steps`** — new batch browser action executor; run click, type, navigate, wait, screenshot, and scroll actions in a single call
- **CLI `chrome` rename** — the `browser` CLI command group is now `chrome`, matching the MCP tool names
- **Shell completions** — `krometrail completions [shell]` generates tab-completion scripts for bash, zsh, and fish

### Fixes

- C# breakpoints with netcoredbg 3.1.3 — fixed via PDB PathMap configuration
- Kotlin and C# adapter bugs resolved
- Fixed missing spawn imports in Swift and C++ adapters
- Screenshot directory now eagerly created when step executor starts a session

### Internal

- Typed error hierarchy, injection deduplication, and session-manager cleanup
- MCP tool handlers standardized with shared helpers and centralized path utilities
- Consolidated duplicated logic across debug adapter layer
- E2E and integration tests added for browser step executor

## v0.2.6

### Features

- **MCP auto-update** — the MCP server now checks for newer versions on every startup and self-updates automatically. Binary installs download from GitHub and atomically replace in place; npx/bunx configs updated to use `@latest` tag; global npm/bun installs run the package manager's update command. Disable with `KROMETRAIL_NO_UPDATE=1`.

## v0.2.5

### Fixes

- Fixed all MCP config examples — `krometrail mcp` (nonexistent subcommand) corrected to `krometrail --mcp` across all docs
- Fixed `claude mcp add` and `codex mcp add` one-liner commands
- Fixed README Quick Start referencing `settings.json` instead of `.mcp.json`

### Features

- Getting Started now leads with the `curl` one-liner installer
- Added focused tool-set one-liners (`--tools debug`, `--tools browser`) to all agent integration guides

### Internal

- Added installation claims test suite (14 tests covering CLI, MCP startup, install script, and doc config regression)

## v0.2.4

### Internal

- Switched npm publish to OIDC trusted publishing (no more stored tokens)
- Added package metadata for npm listing
- Read version from `package.json` at runtime instead of hardcoding

## v0.2.3

### Features

- SEO improvements: robots.txt, XML sitemap, structured data (JSON-LD), privacy policy, FAQ page, accessibility enhancements
- Added OG image and scaled up favicon K mark

## v0.2.2

_Release infrastructure fix — no user-facing changes._

## v0.2.1

### Features

- **Browser annotation API** — lightweight code-placed markers with time-window coalescing
- **CLI agent-friendly overhaul** — namespaced subcommands (`debug launch`, `browser start`), JSON envelope output, structured exit codes, full MCP parity
- **MCP tool filtering** — `--tools debug|browser|session` flag to expose only specific tool groups
- **Curl-based installer** — `curl -fsSL https://krometrail.dev/install.sh | sh` with checksum verification and PATH management
- **"latest" session ID alias** — use `latest` instead of looking up session IDs
- **In-browser marker overlay** — visual markers rendered in the browser during recording
- **Screenshot control panel** — JPEG capture with configurable intervals
- **Browser control MCP tools** — `chrome_start`, `chrome_stop`, `chrome_mark`, `chrome_status`
- **GitHub Pages docs site** — auto-generated tool reference from Zod schemas, Chrome Inspector color palette
- **Agent skills** — split into `krometrail-debug` and `krometrail-chrome` for skilltap

### Fixes

- Fixed `break --clear` CLI bug
- Fixed browser inspect timestamp resolution
- Fixed daemon spawn detection, status-code filtering, and marker IDs
- Fixed react-observer E2E tests
- Fixed CLI command prefixes in docs: added missing `debug` namespace, corrected `session` → `browser`
- Fixed landing page nav bar and 404s
- Removed misleading HH:MM:SS relative timestamp support

### Internal

- Renamed project: agent-lens → bugscope → krometrail
- Type boundary consolidation with central enums module
- CLI E2E journey tests for doctor, commands, debug, and browser workflows
- Browser journey test suite for React and Vue SPAs (69 tests)
- Consolidated adapter prereqs, MCP handlers, and observer base class
- Pattern documentation: registry, zod, errors, adapter-helpers, mcp-handler, test patterns

## v0.1.0 — Initial Development

The initial development period before tagged releases, building the full feature set from scratch.

### Core Debug Loop

- DAP client for debugger communication over TCP and stdio
- Session manager orchestrating launch, attach, breakpoints, stepping, and evaluation
- Viewport renderer producing ~400-token summaries per debug stop
- MCP server exposing all debug operations as tools

### Multi-Language Support

- **Python** (debugpy), **Node.js** (js-debug), **Go** (Delve), **Rust** (CodeLLDB), **Java** (java-debug-adapter), **C/C++** (GDB 14+ / lldb-dap)
- Ruby, C#, Swift, and Kotlin adapters
- Shared adapter helpers: `checkCommand`, `spawnAndWait`, `allocatePort`, `gracefulDispose`, `connectTCP`

### Advanced Debugging

- Conditional breakpoints (`when discount < 0`), hit counts, logpoints
- Exception breakpoint filtering
- Attach mode for running processes
- Multi-threaded debugging with thread/goroutine selection
- Watch expressions (persistent, auto-evaluated on every stop)
- Framework auto-detection for pytest, jest, go test, Django, Flask, mocha

### Browser Observation

- Chrome CDP recorder capturing network, console, DOM mutations, user input, screenshots, storage changes
- React DevTools integration: component lifecycle, state/prop diffs, render counts, bug pattern detection (stale closures, infinite re-renders, missing cleanup)
- Vue Devtools integration: component tracking, Pinia/Vuex store mutations
- Session persistence to SQLite with JSONL event storage
- Investigation tools: `session_search`, `session_inspect`, `session_diff`, `session_replay_context`
- WebSocket lifecycle event capture
- CLS (Cumulative Layout Shift) observer
- Playwright/Cypress test scaffold generation from recorded sessions

### Agent Test Harness

- 35 scenarios across Python, Node.js, and TypeScript at 5 difficulty levels
- MCP, CLI, and baseline comparison modes
- Token usage tracking and reporting
