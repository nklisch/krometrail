---
id: epic-agent-surface-simplification-response-detail-agent-guidance
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-surface-simplification-response-detail
depends_on: [epic-agent-surface-simplification-response-detail-projection]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Replace projection tests and teach progressive response expansion

Replace legacy projection-matrix coverage with focused schema, protocol, target-ranking, diagnostic, temporal-resource, and inline-image tests for the current contract. Delete tests whose only value was preserving removed variants, omitted markers, ancestor closure, or compatibility wrappers.

Rewrite the Krometrail skill and visual-evidence reference so routine calls omit `response`, broader semantic/page inspection requests `expanded`, complete acquired structures request `full`, and pixels use `inline_images: true`. Failed/degraded diagnostics are always actionable and cannot be suppressed. Verify the preflight foundation assertions against the delivered surface and regenerate derived public documentation when source docs change.

## Acceptance evidence

- Runtime, schemas, tests, skill, references, and current foundation docs use only concise/expanded/full and boolean inline image opt-in.
- Skill examples prefer implicit concise and describe expanded/full as deliberate context growth.
- No live source, test, or skill reference remains to removed projection variants or diagnostic suppression; historical changelog entries are left as history.
- Replacement tests protect current external behavior and learned regressions without recreating the deleted combination matrix.
- Documentation generation is byte-current when a source documentation page changes.

## Ordering

Depends on `epic-agent-surface-simplification-response-detail-projection` so examples and tests describe the implemented final structure.
