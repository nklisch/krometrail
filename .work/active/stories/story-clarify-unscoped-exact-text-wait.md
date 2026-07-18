---
id: story-clarify-unscoped-exact-text-wait
kind: story
stage: review
tags: [bug, agent-ux, docs]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Clarify unscoped exact-text waits

## Symptom

An exploratory plugin run used `wait` with a text condition for `Hello World!`, `match_mode: exact`, and no locator. The page visibly contained an exact `Hello World!` heading, but the wait timed out because unscoped exact matching compared against the full observed page text (`observed_length: 122`). Repeating with `match_mode: contains` succeeded immediately. Diagnostic correlation: `4050e515-4569-4ba5-bc22-6936fcd56131`.

## Root cause

The runtime intentionally treats an omitted locator as the full document-body text scope, and `exact` compares the complete normalized text in that scope. The generated MCP schema exposed neither rule, so an agent could reasonably interpret `exact` as “any visible text node equals this value.”

## Fix

Preserve the stable matching behavior and publish the scope and comparison semantics directly on the generated `locator` and `match_mode` schema fields. The guidance should explicitly recommend a locator for exact element text and `contains` for an unscoped substring.

## Regression

A schema test must prove the public `wait` tool exposes both descriptions after local references are expanded.

## Implementation notes

- Added field descriptions to the schema-generating wire contract without changing matching behavior.
- Added the same operational guidance to the shipped Krometrail skill.
- Verified core wait serialization, the published MCP schema regression, and plugin distribution contracts.
