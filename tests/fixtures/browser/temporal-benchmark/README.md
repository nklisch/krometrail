# Temporal benchmark target

This is a dependency-free standalone browser target for Krometrail's temporal-evidence
qualification. It is not part of the Krometrail runtime and does not expose framework state.

The evaluator selects a case and duration through a relative URL query and clicks the generic
`Run interaction` button:

```text
index.html?case=<case-id>&duration_ms=<16|33|50|100|200>
```

The page has no network dependency, external assets, wall-clock source, or random source. Visual
updates use the browser's monotonic `performance.now()` value and `requestAnimationFrame`. The
requested duration is an intended fixture interval; it is not evidence that Chrome presented or
Krometrail captured every display frame.

The evaluator-owned case phases and expected final states are committed separately in
`docs/evidence/temporal-evaluation/v1/benchmark-definition.json`. The page intentionally does
not render case labels, phase IDs, defect labels, or ground-truth output.
