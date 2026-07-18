# Lifecycle-Complete Browser Overrides

A persisted target-scoped browser override must have complete initial-apply, inverse-clear, rollback, and reconnect-replay behavior before acknowledged state or capture proceeds.

## Rationale

Browser emulation is external mutable state. A partial command sequence can leave the browser and Krometrail's acknowledged target state disagreeing, especially after navigation, failed application, clear, or transport reconnect. Treat every facet of an override as one lifecycle contract.

## Examples

- `crates/krometrail-cdp/src/session/operations.rs:321` — initial viewport application completes its CDP command sequence before persisting acknowledged state.
- `crates/krometrail-cdp/src/session/operations.rs:551` — failed application restores the prior complete override rather than only the first command.
- `crates/krometrail-cdp/src/session/runtime.rs:237` — navigation replay restores persisted target-scoped state before normal operation resumes.
- `crates/krometrail-cdp/src/session/reconnect.rs:345` — reconnect replay is bounded per target and failure remains target-local.
- `crates/krometrail-cdp/src/control/viewport.rs:48` — apply and clear cover mobile page scale together with metrics and touch state.

## When to Use

Use for viewport, device metrics, touch, page scale, user-agent, locale, timezone, geolocation, or any other browser-side override that survives in target state.

## When Not to Use

Do not persist one-shot commands or transient observations as overrides. Do not model a browser-native value as an override merely because it was observed after a command.

## Common Violations

- Persisting acknowledged state before every external command succeeds.
- Clearing only device metrics while leaving touch or page scale active.
- Replaying on navigation but not reconnect, or reconnect but not navigation.
- Continuing capture after replay failure as though state were restored.
- Rolling back to defaults instead of the target's prior acknowledged override.
