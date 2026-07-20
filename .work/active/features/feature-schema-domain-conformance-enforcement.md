---
id: feature-schema-domain-conformance-enforcement
kind: feature
stage: implementing
tags: [agent-ux, testing, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Schema/domain conformance enforcement

## Brief

The published MCP schema and the domain that validates against it are generated
from two independent sources, and nothing forces them to agree. The 2026-07-20
sixth shakedown found three separate defects of this one family, and a fourth
(`IntervalAnchorScope`) was fixed by hand in v1.2.6 without addressing the cause.

The concrete root cause: `stable_registry!`
(`crates/temporal-vision/src/lib.rs:3`) hand-writes `Serialize`/`Deserialize`
from `$wire` snake_case literals but derives `schemars::JsonSchema` on the bare
enum. schemars has no `#[serde(rename_all)]` to read, so it falls back to the
Rust variant identifiers and emits PascalCase. Every enum declared through that
macro publishes variant names the deserializer rejects.

Nine enums go through it: `ArtifactKind`, `ErrorCode`, `EvidenceClass`,
`FrequencyMode`, `NormalizationKind`, `PixelFormat`, `RegionCoordinateSpace`,
`SelectionReason`, `TimePalette`. `ErrorCode` and `ArtifactKind` appear in
**responses**, so Krometrail currently emits responses that violate its own
published schema — every observed error carried `"code":"resource_limit_exceeded"`
while the schema declares `ResourceLimitExceeded`.

This feature makes the casing subclass unrepresentable and installs a mechanical
guard for the broader family. It is deliberately sequenced ahead of
`feature-wire-contract-corrections`, whose scope its conformance test will size.

## Why a casing fix alone is insufficient

The family is wider than casing. Of the three defects found live, a casing rule
catches one:

| Defect | Casing rule catches? | Conformance test catches? |
|---|---|---|
| `frequency_mode` Pascal vs snake | yes | yes |
| `region_filmstrip` rejects advertised `fit_limits` | no | yes |
| anchor scope optional-vs-required (fixed v1.2.5) | no | yes |

The `fit_limits` case is instructive: the schema is arguably correct and the
domain is wrong. A conformance test does not care which side is at fault — it
forces reconciliation. That is the discipline that has been missing.

The invariant to enforce:

> Every input the published schema declares valid must be accepted by the
> domain, and every response emitted must validate against its published schema.

## Simplification opportunity

- Collapses three ways of declaring a wire enum into one sanctioned door.
- Retires hand-written per-type schema assertions where the generative
  conformance test subsumes them. `generated_video_schemas_publish_strict_wire_shapes_and_hard_bounds`
  and the `IntervalAnchorScope` assertion added in v1.2.6 should be reviewed at
  design time: keep those asserting genuine domain intent (hard bounds,
  strict-vs-tolerant branches), delete those merely restating what the
  conformance sweep now proves universally.
- Removes the need for future contributors to remember schema assertions at all.

## Design intent (four tiers)

Recorded from the agreed design discussion; `feature-design` owns the details.

**Tier 1 — make it unrepresentable.** Hand-implement `JsonSchema` inside
`stable_registry!` from the same `$wire` literals that already drive serde. Then
schema and serde are emitted from one token sequence and divergence cannot be
written. This is stronger than a compile error: there is no wrong state to
detect. Then make the macro the only sanctioned wire-enum declaration and fold
the two plain-derive outliers `RetentionPolicy` and `CaptureGapPolicy`
(`crates/krometrail-core/src/timeline/range.rs:167,172`) into it.

**Tier 2 — compile-time enforcement that the door is used.** A sealed `WireEnum`
marker trait, implementable only by the macro, so generic wire boundaries bounded
on `T: WireEnum` reject hand-rolled enums at compile time. **This is explicitly
partial, not total** — it binds only where the boundary is generic; a struct
field typed concretely still compiles. Making it total would need a proc-macro
owning the whole request/response surface, which is not judged worth the weight.
Design should confirm how much of the wire surface can actually route through
the bound before committing to this tier.

**Tier 3 — generative conformance test.** The tier that kills the family.
`crates/krometrail-mcp/src/registry.rs` already enumerates every tool. Walk the
generated schema for each, and for every enum and `oneOf` branch construct a
minimal instance and assert it deserializes. Cannot be compile-time — it
exercises domain validation, which runs at runtime by definition — but it is
mechanical and total, which is the next best thing.

**Tier 4 — regression guard.** A grep/AST check in the existing quality gate:
no `#[derive(..., JsonSchema)]` on an enum outside the macro. Catches the "someone
added a new one the old way" case that tiers 1–3 do not.

## Expected scope discovery

Tier 3 will almost certainly surface mismatches beyond the two found by hand —
the shakedown sampled the surface, it did not sweep it. That is the point of the
tier, but it means `feature-wire-contract-corrections` cannot be sized until this
lands. Findings that are pure corrections belong to that feature; findings that
reveal a further structural gap belong here.

## Risks

- Tier 1 changes published schema text for nine enums. Under Current Contract
  Discipline that is acceptable, but it is a real external-contract change and
  should be stated plainly rather than framed as a no-op.
- Folding `RetentionPolicy`/`CaptureGapPolicy` into the macro changes accepted
  wire values (`AllowPartial` → `allow_partial`). This is a genuine breaking
  input change, not cosmetic. It is the reason this feature is not tagged
  `[refactor]`.
- Tier 2 risks being ceremony if little of the surface is generic. Design should
  be willing to drop it and rely on tiers 1, 3, and 4 rather than build a bound
  nothing uses.
- Tier 3 must construct instances that exercise *domain* validation, not merely
  serde. A test that only round-trips serde would have passed on the `fit_limits`
  defect and taught us nothing.

## Foundation docs

`docs/SPEC.md` is the authoritative external-contract document and does not
currently state the conformance invariant. Design should decide whether the
invariant belongs there; if so it rolls forward in the same stride.

Origin: 2026-07-20 sixth shakedown against v1.2.6.

## Design decisions

- **Tier 2 (sealed `WireEnum` trait) is DROPPED.** The scope brief flagged this as
  partial and told design to confirm there was a real enforcement point before
  committing. There is not. Schemas are generated through
  `type_input_schema::<T>()` (`crates/krometrail-mcp/src/registry.rs:163-263`),
  where `T` is the *request struct*, not the enum. A bound on individual wire
  enums would have nothing to check it — the generic boundary sits a level above
  where the bound would apply. Building it would be pure ceremony. Tier 1 makes
  divergence unrepresentable for macro enums and tier 4 catches enums declared
  outside the macro, which covers the ground tier 2 was meant to cover.

- **Preserve schema `description` when hand-implementing `JsonSchema`.** The
  current `derive(JsonSchema)` picks up doc comments and publishes them as
  `description` (e.g. `FrequencyMode`'s "Quantity encoded as brightness in the
  change-frequency panel"). A naive hand impl would silently drop them and
  regress the agent surface. The macro will match `#[doc = $doc:literal]`
  explicitly and reassemble the text with `concat!`, so descriptions survive.

- **Rejected the one-line alternative.** Adding `#[serde(rename_all = "snake_case")]`
  to the derived enum would fix all nine today, because every current `$wire`
  literal happens to equal the snake_case of its identifier. It is rejected
  because it re-breaks silently the first time a variant needs a wire name that
  is not snake_case of its ident — which is the entire reason the `$wire` literal
  exists. Deriving from `$wire` is correct by construction; `rename_all` is
  correct by coincidence.

- **`fit_limits` cannot be caught by this feature's test, and that is the point.**
  See the honest-limits section below. The resolution is to make schema the
  source of truth so domain-only rules stop existing.

## Architectural choice

**Chosen: single-source generation in the macro, plus a schema-driven conformance
sweep.** The macro already holds both halves of the contract (`$variant` and
`$wire`); it simply feeds them to two different consumers. Emitting `JsonSchema`
from the same `$wire` literals that drive serde collapses that to one source.

Considered and rejected:

- *Fix the nine enums individually.* Treats the symptom. The tenth enum
  reintroduces the bug.
- *Generate the schema from serde at runtime and diff it in a test.* Catches
  divergence but permits it to exist between commits, and needs a second
  mechanism anyway for enums declared outside the macro.

## Honest limits of the conformance sweep

This matters enough to record explicitly, because the scope brief implied broader
coverage than is achievable and implementation should not chase it.

The sweep operates at the **deserialization** boundary. It can prove that every
value the schema advertises is accepted by serde. It cannot prove acceptance by
domain logic that runs *after* deserialization:

| Defect | Layer | Sweep catches? |
|---|---|---|
| `frequency_mode` Pascal vs snake | deserialization | yes |
| anchor scope optional-vs-required | deserialization | yes |
| `region_filmstrip` rejects `fit_limits` | post-deserialization domain rule | **no** |

`fit_limits` deserializes fine; it is rejected later inside artifact generation.
Catching that class in a unit test would require invoking the domain, which for
most tools needs a live browser session — not viable as a gate test.

**The resolution is a contract rule, not more test machinery:** if the domain
rejects an input unconditionally, the schema must not advertise it. Domain-only
restrictions are the bug. Once `feature-wire-contract-corrections` narrows the
`region_filmstrip` schema to the variants it actually accepts, the rule becomes
schema-expressible and the sweep guards it forever after. This feature supplies
the mechanism; that feature applies it.

## Implementation Units

### Unit 1: Emit `JsonSchema` from the wire literals
**File**: `crates/temporal-vision/src/lib.rs`

Replace `schemars::JsonSchema` in the derive list with a hand-written impl inside
`stable_registry!`, generated from the same `$wire` literals as serde. Capture doc
comments explicitly so `description` survives.

```rust
macro_rules! stable_registry {
    (
        $(#[doc = $doc:literal])*
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($variant:ident => $wire:literal),+ $(,)?
        }
    ) => {
        $(#[doc = $doc])*
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum $name { $($variant),+ }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> String { stringify!($name).to_owned() }
            // enum values MUST come from $wire, never from stringify!($variant)
            // description assembled from the captured $doc literals
        }
        // ... existing as_str / Display / Serialize / Deserialize unchanged
    };
}
```

**Implementation Notes**:
- The doc-capture arm must precede `$(#[$meta])*` or the matcher will consume doc
  attributes as generic meta and `concat!` will have nothing to work with.
- `schema_name()` keeps the Rust identifier — that is a type name, not a wire
  value, and changing it would churn `$ref` targets for no benefit.
- Verify against the schemars version already in the lockfile; the `JsonSchema`
  trait shape differs between 0.8 and 1.x. Match whatever the existing derive
  produces so output stays byte-comparable where nothing should change.

**Acceptance Criteria**:
- [ ] All nine macro enums publish their `$wire` values in `enum`, not the Rust
      identifiers.
- [ ] `FrequencyMode`'s published `description` is unchanged from today.
- [ ] Emitted responses containing `ErrorCode` / `ArtifactKind` validate against
      the published schema — specifically `"code":"resource_limit_exceeded"` is
      now schema-valid.
- [ ] Adding a variant whose `$wire` is deliberately not snake_case of its
      identifier still produces agreeing schema and serde.

### Unit 2: Fold the plain-derive outliers into the macro
**File**: `crates/krometrail-core/src/timeline/range.rs`

Move `RetentionPolicy` (`:167`) and `CaptureGapPolicy` (`:172`) into
`stable_registry!` with snake_case wire literals, so one door remains.

**Implementation Notes**:
- This changes accepted wire values: `AllowPartial` → `allow_partial`,
  `RequireComplete` → `require_complete`, `Include` → `include`,
  `Reject` → `reject`. Breaking input change, accepted under Current Contract
  Discipline. No alias, no dual-accept.
- `stable_registry!` currently lives in `temporal-vision`. Using it from
  `krometrail-core` requires either exporting it or relocating it. Prefer
  relocating to `krometrail-core` — it is a domain contract mechanism and
  `temporal-vision` already depends inward, matching `injected-core-ports`.
  Confirm no dependency inversion before moving.
- Update every in-repo call site and any fixture or doc using the old spellings.

**Acceptance Criteria**:
- [ ] Both enums declare through the macro; no `derive(JsonSchema)` remains on
      either.
- [ ] Schema and serde both publish/accept only the snake_case forms.
- [ ] Repo-wide search finds no remaining `AllowPartial` / `RequireComplete` /
      `Include` / `Reject` wire spellings in requests, fixtures, or docs.

### Unit 3: Generative schema/serde conformance sweep
**File**: new test module under `crates/krometrail-mcp/`

Walk every registered tool's input schema; for each string `enum` and each
`oneOf` branch, construct a minimal instance and assert it deserializes into the
tool's request type.

**Implementation Notes**:
- `registry.rs:295-361` already contains a pass that materializes every tool's
  schema for validation — reuse that enumeration rather than writing a second
  one; it is the existing single source of tool identity.
- Minimal-instance construction must fill all `required` properties. For nested
  `oneOf`, pick the first branch deterministically so failures are reproducible.
- The assertion is *acceptance*, not round-trip equality. A value that
  deserializes into a different-but-valid variant is not a failure; a value that
  fails to deserialize is.
- Test must name the offending tool, JSON pointer, and value on failure. A bare
  "conformance failed" would be worse than the hand-written assertions it
  replaces.

**Acceptance Criteria**:
- [ ] Every advertised string-enum value across every tool deserializes.
- [ ] Every `oneOf` branch across every tool deserializes.
- [ ] Reverting Unit 1 makes this test fail, naming `frequency_mode`.
- [ ] Failure output identifies tool, pointer, and value.

### Unit 4: Gate guard against new plain derives
**File**: quality-gate scripting

Reject `#[derive(..., JsonSchema)]` on an `enum` outside `stable_registry!`.

**Implementation Notes**:
- Structs are unaffected — `rename_all = "snake_case"` on the 185 struct types is
  correct and consistent. This guard is enum-only.
- A grep-level check is acceptable; a false positive is cheap to waive and the
  maintenance cost of AST tooling is not justified here.

**Acceptance Criteria**:
- [ ] Adding a plain `derive(JsonSchema)` enum fails the gate with a message
      naming the file and pointing at `stable_registry!`.
- [ ] No false positive on any existing struct.

## Implementation Order

1. Unit 1 (macro emits schema from wire literals)
2. Unit 2 (fold outliers — depends on Unit 1's macro shape)
3. Unit 3 (conformance sweep — must be able to fail against pre-Unit-1 code)
4. Unit 4 (gate guard — independent, last)

No child stories. The four units are one cohesive delivery for a single owner
with no useful intermediate checkpoint: Unit 3 is the acceptance evidence for
Units 1–2, and splitting them would mean landing a guard that cannot yet pass.
Spawning stories here would manufacture worker targets without adding a design
checkpoint.

## Testing

- **Unit 3 is the primary test surface** and is itself deliverable — it protects
  the whole schema/serde contract, not one instance.
- **Regression assertion**: reverting Unit 1 must make Unit 3 fail on
  `frequency_mode`. Without this the sweep could silently pass vacuously (e.g. if
  it enumerates zero enums), which is the main way this kind of test rots.
- **Vacuity guard**: assert the sweep visits a non-zero, plausible count of
  enums and `oneOf` branches. A sweep that walks nothing passes everything.
- **Test removal**: review `generated_video_schemas_publish_strict_wire_shapes_and_hard_bounds`
  and the `IntervalAnchorScope` schema assertion added in v1.2.6. Keep the parts
  asserting genuine domain intent (hard numeric bounds, strict-vs-tolerant
  branch split); delete the parts that merely restate what the sweep now proves
  universally. Do not delete wholesale.

## Risks

- **Riskiest assumption: schemars' `JsonSchema` trait can be hand-implemented
  cleanly at the pinned version.** If its shape is more awkward than expected,
  the fallback is `#[schemars(rename_all = "snake_case")]` on the derive plus a
  macro-generated compile-time assertion that each `$wire` equals the snake_case
  of its identifier — preserving the guarantee while keeping the derive. Weaker
  (it forbids non-snake_case wire names rather than supporting them) but it
  holds the invariant.
- Unit 2's macro relocation could invert a crate dependency. If `krometrail-core`
  cannot host the macro without inverting, leave it in `temporal-vision` and
  export it rather than forcing the move.
- Unit 3 may surface a large set of mismatches. That is its purpose, but they
  belong to `feature-wire-contract-corrections`, not here. Resist fixing them in
  this feature beyond what Units 1–2 fix structurally; record them for sizing.
- Unit 4's grep could false-positive on doc comments or test fixtures mentioning
  the derive. Scope it to non-test source and accept a waiver mechanism.
