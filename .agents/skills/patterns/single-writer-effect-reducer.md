# Single-Writer Reducer with Explicit Effects

Reduce serialized lifecycle inputs into new state plus explicit effects, then execute effects outside the reducer.

## Rationale

Target identity, lifecycle transitions, reconnect generations, and capture ownership stay deterministic and single-writer while asynchronous transport work remains outside state mutation.

## Examples

- `crates/krometrail-cdp/src/targets/reducer.rs:1` — the reducer explicitly excludes async work, clocks, randomness, and transport handles.
- `crates/krometrail-cdp/src/targets/model.rs:224` — `SupervisorInput` enumerates lifecycle inputs.
- `crates/krometrail-cdp/src/session/operations.rs:611` — an input is reduced, committed, then effects are handled.
- `crates/krometrail-cdp/src/session/runtime.rs:222` — runtime reduction replaces state and extends an explicit effect queue.

## When to Use

Use for asynchronous lifecycle state machines where stale inputs, ordering, generations, and ownership must be explicit.

## When Not to Use

Avoid for ordinary request/response functions or simple synchronous state.

## Common Violations

- Sending transport commands inside the reducer.
- Mutating state from multiple tasks.
- Applying stale-generation inputs.
- Deriving identity from mutable URLs or titles.
