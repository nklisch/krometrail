---
id: durable-agent-diagnostics
kind: feature
stage: drafting
tags: [browser, storage, agent-ux, security]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Durable agent diagnostics

## Brief

Give every Krometrail installation a durable, bounded, private diagnostic log that remains easy to locate regardless of the project directory from which the MCP server is used. The binary currently emits structured `tracing` events throughout lifecycle, CDP, capture, storage, and shutdown code but installs no tracing subscriber, so those events are normally discarded and agents can report only generic public errors and aggregate counters.

Initialize diagnostics before storage and browser composition, write sanitized structured events beneath the platform Krometrail data directory, and rotate or prune them under a fixed bound. Preserve the concrete failure stage and safe causal classification for capture, persistence, transport, observation, and shutdown failures. Do not log page content, screenshots, image bytes, form values, secrets, raw protocol payloads, or unredacted URLs.

Expose enough stable diagnostic context in MCP responses for an agent to find the correct evidence without guessing: a correlation identifier for failed/degraded operations, the active diagnostic-log path, and concise collection guidance. Update the Krometrail skill so an agent working in any repository knows when and how to inspect the bounded tail around that identifier, summarize the relevant sanitized events, and include version/platform/session/capture context in a later issue report without copying the entire log.

The file log is supplemental evidence. Stdout remains exclusively JSON-RPC, stderr remains useful for startup failures that occur before file logging is available, public structured errors remain actionable without log access, and diagnostic logging must never become a reason for browser control or MCP startup to fail.

## Strategic decisions

- **Availability**: create the bounded private diagnostic log by default so failures discovered after a walkthrough remain debuggable without prior opt-in.
- **Location**: place diagnostics under Krometrail's platform data directory, independent of the caller's current working directory, and expose the resolved path through the agent-facing surface.
- **Privacy**: retain operational metadata and sanitized causal classifications only; browser content, user input, media, raw CDP payloads, and unredacted URLs are outside the log contract.
- **Agent workflow**: expose correlation IDs and teach targeted excerpt collection; do not encourage agents to attach or paste whole logs.

## Simplification opportunity

Centralize diagnostic initialization, redaction, correlation, and retention at the composition root instead of adding issue-specific stderr messages or ad hoc files across adapters. Replace documentation that describes stderr as the only diagnostic destination while retaining stderr as the pre-initialization fallback.
