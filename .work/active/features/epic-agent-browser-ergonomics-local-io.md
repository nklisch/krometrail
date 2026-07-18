---
id: epic-agent-browser-ergonomics-local-io
kind: feature
stage: drafting
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Explicit clipboard and download workflows

## Brief

Add bounded explicit clipboard read/write operations and a managed-download lifecycle with completion metadata, cancellation, and canonical local resource access. Define managed-versus-attached authority, a session-owned download directory, cleanup/retention behavior, and focus or permission failures precisely. Content and local paths never enter browser-event evidence, diagnostics, or ordinary status.

The feature excludes arbitrary filesystem destinations, implicit clipboard access, download uploads, response-body capture, and silent permission escalation. Unsupported platforms or externally owned sessions fail with stable recovery guidance.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent local I/O capability with elevated privacy and ownership risk

## Simplification opportunity

Use the existing operation registry for explicit mutability, browser-event pipeline for bounded lifecycle signals, and canonical resource layer for completed bytes. Keep paths and clipboard content out of shared diagnostics and timeline payloads.

## Foundation references

- `docs/SPEC.md` — Browser-Control Surface and Local Data and Telemetry
- `docs/ARCHITECTURE.md` — MCP Boundary, Failure Isolation, and Observability
