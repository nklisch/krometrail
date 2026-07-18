---
id: gate-security-isolate-clipboard-execution-world
kind: story
stage: drafting
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
