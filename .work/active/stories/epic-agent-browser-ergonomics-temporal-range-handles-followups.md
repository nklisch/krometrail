---
id: epic-agent-browser-ergonomics-temporal-range-handles-followups
kind: story
stage: implementing
tags: [agent-ux, visual]
parent: epic-agent-browser-ergonomics-temporal-range-handles
depends_on: [epic-agent-browser-ergonomics-temporal-range-handles-authority]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Handle-enabled temporal follow-ups

## Checkpoint

Publish a bundle range handle, accept exactly one handle-or-range across artifact, region, source-list/fetch, browser-event, pinning, and video routes, normalize to the existing validated range contracts, echo the handle in responses, and teach agents to reuse it.

## Acceptance evidence

- Schema-wide tests prove exclusive additive input without changing legacy range requests or tool limits.
- Stdio tests prove bundle-to-follow-up reuse, exact range forwarding, compact-projection preservation, and no dispatch after invalidation.
- Skill guidance states lifetime, restart, provenance, and fallback semantics accurately.

## Ordering

Depends on `epic-agent-browser-ergonomics-temporal-range-handles-authority`; all routes consume the one injected authority.
