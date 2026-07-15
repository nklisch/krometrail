---
id: configurable-capture-cadence-core-contracts-and-status
kind: story
stage: implementing
tags: [browser, visual, testing]
parent: configurable-capture-cadence
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Establish the typed capture-stride core contract and status projections

## Checkpoint

Make `every_nth_frame` one validated, generated-schema-backed core value shared by launch and
attach requests, recording-session metadata, browser status, and standalone capture status. This
checkpoint owns no CDP behavior and does not add a second configuration source.

## Exact implementation

**Files**:

- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/browser/control.rs`
- existing core/store tests that construct the affected values

Add one public transparent newtype:

```rust
pub const MIN_EVERY_NTH_FRAME: u8 = 1;
pub const MAX_EVERY_NTH_FRAME: u8 = 60;

pub struct EveryNthFrame(NonZeroU8);

impl EveryNthFrame {
    pub fn new(value: u8) -> Result<Self>;
    pub const fn get(self) -> u8;
}

impl Default for EveryNthFrame;
impl<'de> Deserialize<'de> for EveryNthFrame;
```

The type serializes as an integer, uses the existing validated-transparent-newtype pattern, and
publishes integer minimum/maximum/default metadata through `schemars`. `LaunchBrowser` adds a
public `every_nth_frame: EveryNthFrame` under its existing serde default. `AttachBrowser` adds the
same field to its existing endpoint-validation wire projection; `new(endpoint)` defaults it to 1
and `with_every_nth_frame` supports programmatic non-default values.

Add the typed field/getter to `RecordingSession`, `BrowserStatus`, and `TargetCaptureStatus`. Their
constructors and custom wire projections require the value so serialized status/session records
cannot silently lose it. Update existing constructors with `EveryNthFrame::default()` where the
fixture is not intentionally non-default. Do not add any migration reader, alias, `fps`,
`frame_rate`, or inferred gap field.

## Acceptance evidence

- [ ] Newtype construction and JSON deserialization accept exactly 1..=60 and reject 0, 61,
      null, strings, and fractional values.
- [ ] Omitted request fields default to 1 for both `LaunchBrowser` and `AttachBrowser`; non-default
      values round-trip through both public JSON shapes.
- [ ] Generated schemas for the newtype and request objects describe an optional integer field
      with minimum 1, maximum 60, and default 1; no hand-written duplicate schema exists.
- [ ] Browser/session/capture status and recording metadata round-trip with the requested value and
      retain all existing lifecycle/statistics/selection invariants.
- [ ] Existing store/catalog and core tests compile after direct constructor updates; no source
      outside the intended contract/test surfaces is changed.

## Ordering

This is the first checkpoint. CDP capture binding, MCP route tests, and evaluation identity depend
on this core type and its exact serialized shape.
