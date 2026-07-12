---
title: "Privacy Policy"
description: "Krometrail privacy policy — website analytics and local runtime data."
---

# Privacy Policy

*Last updated: 2026-07-12*

## Website analytics

Krometrail's documentation website uses Google Analytics for anonymous website usage statistics, including pages visited, referral source, browser type, approximate region, and time spent. Google Analytics may set cookies. You can opt out with a browser extension or your browser's tracking controls.

## The local runtime

The current Rust executable does not send telemetry, session contents, source code, screenshots, browser events, or derived artifacts to Krometrail. The repository's intended runtime is local-first: captured browser data remains on the user's machine unless the user or a connected agent explicitly reads it through a future local MCP boundary.

The current executable has no browser capture or MCP command surface. Future implementations must update this policy before introducing any network communication.

## Contact

For privacy questions, open an issue on [GitHub](https://github.com/nklisch/krometrail/issues).
