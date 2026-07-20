---
id: feature-schema-domain-conformance-enforcement
kind: feature
stage: drafting
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
