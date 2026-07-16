---
id: gate-docs-refresh-architecture-snapshot-and-mcp-map
kind: story
stage: implementing
tags: [documentation, browser, agent-ux]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: docs
created: 2026-07-15
updated: 2026-07-15
---

# Refresh implemented snapshot boundary and MCP module map

## Drift category
Foundation architecture assertion/path drift

## Location
- Doc: `docs/ARCHITECTURE.md:79-85,131-140,220,278-292`
- Contradicting sources: `crates/krometrail-core/src/browser/observation.rs`; `crates/krometrail-cdp/src/control/snapshot.rs`; current `crates/krometrail-mcp/src/*.rs` tree

## Current doc text

> Snapshot identifiers and actionable node references are deferred browser-control work.

The MCP module map also names nonexistent `registry/`, `tools/`, `resources/`, `schemas/`, and `response/` directories.

## Contradiction

Snapshot generation/reference types and registry behavior are implemented, and MCP uses flat modules such as `registry.rs`, `resources.rs`, `schema.rs`, and `response.rs`.

## Required edit

Roll `docs/ARCHITECTURE.md` forward in place to the implemented snapshot/reference boundary and current flat MCP layout. Preserve intended future-state assertions and add no historical-version prose.
