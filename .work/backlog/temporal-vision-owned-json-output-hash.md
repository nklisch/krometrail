---
id: temporal-vision-owned-json-output-hash
kind: story
stage: backlog
tags: [visual, testing]
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-10-01
updated: 2026-10-01
---

# Accept owned JSON strings when deserializing output hashes

The storyboard presentation tests exposed an existing `OutputHash` deserialization
limitation at `c57dc3f3`: a valid artifact manifest serialized into a
`serde_json::Value` cannot be restored with `serde_json::from_value`. It fails with
`invalid type: string ..., expected a borrowed string` before manifest validation.
JSON text deserialization succeeds for the same manifest.

`crates/temporal-vision/src/provenance.rs` implements `OutputHash::deserialize`
through `<&str>::deserialize`, although the hash retains parsed bytes rather than
a borrowed string. An owned-value round trip can be reproduced with
`serde_json::from_value::<OutputHash>(serde_json::to_value(OutputHash::from_bytes([0; 32])).unwrap())`.

Follow-up: accept borrowed and owned strings while retaining canonical SHA-256
validation. Cover valid owned-value and text round trips plus malformed hashes;
ensure negative manifest tests reach their intended validation rather than failing
early on the hash. Presentation tests use the supported JSON text path and check
a positive round trip before malformed tile cases. This is separate from the
approved original-image rendering addition; no runtime hash changes are included.
