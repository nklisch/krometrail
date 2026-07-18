# Bounded Handoff with Explicit Loss Accounting

Use bounded queues for observation pipelines and convert saturation, closure, and lag into explicit gap or recovery state.

## Rationale

Capture and browser-event streams must not block transport indefinitely, silently lose evidence, or confuse queue acceptance with persistence.

## Examples

- `crates/krometrail-cdp/src/capture/pipeline.rs:573` — full/closed ingestion queues create typed capture-gap reasons.
- `crates/krometrail-cdp/src/events/pipeline.rs:440` — event saturation releases budget and records the allocated drop.
- `crates/krometrail-cdp/src/targets/supervisor.rs:205` — subscriber lag is surfaced with missed revision bounds.

## When to Use

Use for continuous visual/event ingestion, bounded fan-out, or any lossy stream whose omissions affect evidence quality.

## When Not to Use

Do not use for command dispatch where backpressure must be returned directly, or terminal signals requiring out-of-band delivery.

## Common Violations

- Awaiting an unbounded send.
- Discarding `Full`/lag outcomes.
- Treating acknowledged or enqueued data as durable.
- Inferring gaps from ordinal arithmetic.
- Hiding subscriber lag.
