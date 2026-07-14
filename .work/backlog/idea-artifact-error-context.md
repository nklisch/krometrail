---
id: idea-artifact-error-context
created: 2026-07-14
updated: 2026-07-14
tags: [visual, agent-ux]
---

Thread Krometrail session, target, and where safe frame identity into artifact decode, epoch, generation, source-loss, deletion, and corruption errors. The current errors are source-safe and stable but often lack `ErrorContext`, so the first bundle/MCP consumer may receive a useful message without enough scope to relate it to an investigation. Add context without exposing encoded bytes, filesystem paths, cache internals, or sensitive provenance values, and preserve the existing error codes/retry semantics.
