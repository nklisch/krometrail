---
id: gate-security-enforce-private-storage-permissions
kind: story
stage: drafting
tags: [security, storage, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: security
created: 2026-07-15
updated: 2026-07-15
---

# Enforce private permissions for local evidence and managed profiles

## Severity
Medium

## Domain
Local evidence privacy and profile protection

## Location
`crates/krometrail-store/src/index/mod.rs:60`

## Evidence

Recording/index/segment and managed-profile roots are created with platform defaults. On common Unix umasks this can leave directories traversable and evidence files readable by other local users even though they may contain screenshots, browser events, and profile data.

## Remediation direction

Apply owner-only directory and file permissions on supported Unix platforms at the creation boundary, validate existing managed roots, and preserve explicit operator-owned custom paths. Define a proportional cross-platform policy without introducing remote encryption, authentication, or disabling local evidence storage.
