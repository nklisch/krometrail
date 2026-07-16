---
id: gate-security-validate-initial-browser-url
kind: story
stage: drafting
tags: [security, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: security
created: 2026-07-15
updated: 2026-07-15
---

# Prevent Chrome switch injection through initial_url

## Severity
Medium

## Domain
Input validation and browser launch

## Location
`crates/krometrail-cdp/src/launcher/startup.rs:227`

## Evidence

`LaunchBrowser.initial_url` is an externally generated MCP request string and is appended directly as a Chrome argument. A value beginning with `-` is passed without shell expansion but can still be interpreted by Chrome as a command-line switch rather than a navigation URL.

## Remediation direction

Validate the initial navigation value at the core external boundary against the supported URL grammar and reject switch-like or unsupported values before launcher invocation. Preserve the useful initial-page capability and generated MCP contract; do not remove navigation or add a parallel launcher-only validator.
