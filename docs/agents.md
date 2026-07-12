# Documentation Navigation

## Authoritative foundation

These documents define Krometrail’s current direction and intended system:

- **[VISION.md](VISION.md)** — purpose, product thesis, audience, and boundaries.
- **[SPEC.md](SPEC.md)** — externally observable behavior and system contracts.
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Rust workspace, component boundaries, data flow, storage, and failure isolation.
- **[VISUAL-EVIDENCE.md](VISUAL-EVIDENCE.md)** — temporal artifact vocabulary, provenance, and interpretation rules.
- **[EVALUATION.md](EVALUATION.md)** — capture, artifact, browser-control, and agent-effectiveness validation.

Read these five documents before using other project documentation to understand the system.

## Other documentation

Documentation outside the authoritative foundation does not define the current system unless one of the five foundation documents links to it explicitly. Git tag `v0.2.20` preserves the reference implementation and documentation for the DAP debugger, browser-event recorder, DOM observation, and framework-state integrations.

Generated documentation under `docs/.generated/` and material under `docs/legacy/` are not sources of current behavior. Historical design documents remain available at Git tag `v0.2.20`.
