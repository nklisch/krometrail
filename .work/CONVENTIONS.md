# Project Conventions

## Release mapping

tag-based

## Tag taxonomy

- `visual` — temporal frame analysis, visual measurements, and generated artifacts.
- `browser` — Chrome lifecycle, CDP transport, capture, structured snapshots, and browser control.
- `storage` — timeline indexing, frame segments, retention, recovery, and artifact persistence.
- `agent-ux` — MCP tools, responses, resources, and agent-facing workflows.
- `infra` — builds, packaging, distribution, CI, and development infrastructure.
- `security` — local data safety, browser-control boundaries, validation, and dependency security.
- `testing` — automated tests, fixtures, benchmarks, and evaluation infrastructure.
- `perf` — throughput, latency, memory, capture overhead, or storage efficiency; routes to `perf-design`.
- `refactor` — behavior-preserving structural change only; any observable caller-facing change means this is not a refactor; routes to `refactor-design`.
- `prose` — no-code documentation, conventions, copy, or configuration-as-prose; routes to `prose-author`.
- `research` — grounded research input rather than a shippable deliverable; routes to `agentic-research:research-orchestrator`, carries commissioning `research_dials`, does not bind to a release, and runs verification inline.

## Slug conventions

Use kebab-case. Prefix child feature and story slugs with their parent slug.

## Stage overrides

None. Use the standard agile-workflow stages.

## Terminal-tier retention

delete-refs

## Gate config

```yaml
gates_for_release: [security, tests, cruft, docs, patterns]
gate_finding_routing:
  critical: implementing
  high: implementing
  medium: drafting
  low: backlog
  info: skip
gate_refactor_scan_library_roots:
  - .agents/skills
  - .claude/skills
binding_guard: warn
epic_cohesion: phased
backlog_staleness_days: 90
```
