---
id: idea-restore-rust-1-85-contract
created: 2026-07-14
updated: 2026-07-14
tags: [infra, bug]
---

The MCP SDK qualification exposed that the current workspace no longer satisfies its declared Rust 1.85 contract before MCP is added. The committed lock selects ICU/idna packages declaring Rust 1.86, and current source includes let-chain syntax rejected by Rust 1.85 (including browser batch/wait code and temporal provenance). Restore a locked `cargo +1.85.0 check --workspace --all-targets --locked` gate without raising the declared MSRV; keep dependency and syntax corrections behavior-preserving and verify the normal workspace gates remain green.
