---
id: idea-mcp-cancellation-protocol-regression
created: 2026-07-14
updated: 2026-07-14
tags: [testing, agent-ux]
---

Add a focused MCP-layer protocol regression for request cancellation. Drive a real JSON-RPC `notifications/cancelled` message through the existing in-memory rmcp service while a fake `BrowserSessionPort::execute` is blocked, then assert the rmcp request token reaches `McpCancellation`/`BrowserOperationContext`, the operation returns caller-visible `cancelled`, and another request/session remains unaffected. Inner CDP cancellation and adapter wiring are currently covered separately; this would protect their cross-layer composition from future drift.
