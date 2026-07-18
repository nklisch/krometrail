---
id: gate-security-isolate-clipboard-execution-world
kind: story
stage: done
tags: [security]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: security
created: 2026-07-18
updated: 2026-07-18
---

# Isolate clipboard execution from page-script monkeypatching

## Severity
Medium

## Domain
Data Protection / API Security

## Location
`crates/krometrail-cdp/src/control/clipboard.rs:22`

## Evidence

```rust
"Runtime.callFunctionOn",
json!({"functionDeclaration": READ_CLIPBOARD, "awaitPromise": true, "returnByValue": true}),
```

The fixed bridge currently executes in the selected page's default JavaScript realm, where hostile page code can replace `navigator.clipboard.readText` or `writeText`, observe write plaintext, or fabricate reads.

## Remediation direction

Invoke the fixed clipboard bridge in an isolated execution world whose built-ins cannot be monkeypatched by page scripts while preserving Chrome's focus, permission, visibility, and secure-context enforcement.

## Acceptance

- Each explicit clipboard call resolves the current main document frame, creates a fixed-name document-scoped isolated world, and runs only the fixed bridge in its returned execution context.
- Clipboard plaintext remains a CDP value argument/result only; it is never interpolated into function declarations, world names, logs, or diagnostics.
- The bridge still checks native secure-context, document visibility/focus, Clipboard API availability, and Chrome permission outcomes; Krometrail sends no permission or focus mutation command.
- Navigation/document replacement during world resolution fails as `stale_reference`; transport closure remains `browser_disconnected`.

## Tests

Scripted CDP tests assert `Page.getFrameTree` → `Page.createIsolatedWorld` → `Runtime.callFunctionOn` ordering, exact frame/context routing, fixed bridge/value separation, no escalation commands, and stale response handling.

## Implementation and review

Clipboard calls now resolve the current main frame, create the fixed `__krometrail_clipboard_v1` isolated world, and pass its execution-context ID to the fixed bridge. Native secure-context, focus, visibility, API, and permission checks remain inside that world; plaintext remains only a CDP argument/result. Missing frame/world and destroyed-context transport failures fence as stale while disconnect classification is preserved. Scripted tests pass 4/4 and CDP all-target clippy passes. Bounded inline review confirmed command ordering, no universal access, no permission/focus mutation, and no page-realm function or value interpolation. Verdict: pass.
