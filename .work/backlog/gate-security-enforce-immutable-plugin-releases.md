---
id: gate-security-enforce-immutable-plugin-releases
tags: [security, distribution]
release_binding: null
gate_origin: security
created: 2026-07-16
updated: 2026-07-16
---

# Enforce immutable plugin binary releases

## Severity

Medium

## Domain

Supply chain and release workflow

## Location

`plugin/scripts/install-managed.sh:197`

## Evidence

The managed installer verifies the binary against `checksums.txt` from the same GitHub release. Without immutable releases, a compromised release publisher can replace both while preserving the tag and version identity.

## Design

Use GitHub's repository-level immutable-release control as the provenance boundary for all future managed binaries. Adapt the release workflow to the required draft-first lifecycle: build and attest every platform asset, create one draft with all assets and checksums, publish only after the complete set is attached, then require GitHub's signed release attestation and unchanged tag identity. Enable repository release immutability before publishing v1.0.1. Keep installer-side exact checksum and identity checks as corruption and platform-selection defenses.

This closes post-publication replacement without adding an unavailable client-side verifier dependency to the POSIX launcher. A compromise before publication remains a build/publisher compromise and is addressed by pinned actions, exact tag checkout, per-asset build attestations, and the signed immutable-release attestation.

## Acceptance evidence

- Repository immutable releases are enabled and queried successfully before v1.0.1 publication.
- The workflow creates a draft, uploads the complete asset set, then publishes it.
- Publication verifies the signed release attestation and exact tag identity.
- Static distribution contracts reject regression to direct mutable publication.

## Disposition

Operator-deferred during the v1.0.1 security gate. The current exact checksum, executable identity, pinned workflow actions, immutable source tag checks, and published build attestations remain the accepted release boundary. Repository-level immutable releases can be enabled later with a draft-first workflow migration.
