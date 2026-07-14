---
title: "Privacy Policy"
description: "Krometrail privacy policy — website analytics and local runtime data."
---

# Privacy Policy

*Last updated: 2026-07-14*

## Website analytics

Krometrail's documentation website uses Google Analytics for anonymous website usage statistics, including pages visited, referral source, browser type, approximate region, and time spent. Google Analytics may set cookies. You can opt out with a browser extension or your browser's tracking controls.

## The local runtime

The Rust executable does not send telemetry, session contents, source code, screenshots, browser events, or derived artifacts to Krometrail. Browser control, recording storage, and MCP stdio transport run locally. An explicitly attached Chrome-compatible or Electron renderer endpoint must be local.

Captured browser data remains on the user's machine unless the user or connected agent explicitly reads an image or structured result through MCP. Standard output is reserved for MCP protocol traffic; diagnostics do not include browser content or image bytes.

## Contact

For privacy questions, open an issue on [GitHub](https://github.com/nklisch/krometrail/issues).
