---
id: idea-upload-symlink-policy
created: 2026-07-14
updated: 2026-07-14
tags: [browser, security]
---

Decide and document the upload symlink policy explicitly. The current local-first browser-control boundary canonicalizes operator-supplied paths and follows symlinks before sending them to Chrome; an earlier implementation design mentioned rejecting symlink escapes without defining an allowed root. Either retain the current local-operator-authority contract and state it clearly, or introduce an explicit configured upload root with containment checks. Do not add an implicit working-directory root or claim containment that the runtime does not enforce.
