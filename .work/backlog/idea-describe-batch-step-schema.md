---
id: idea-describe-batch-step-schema
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux, browser]
---

The v1.0.4 `batch` MCP tool advertises `steps` to the agent as
`Array<unknown | unknown | ...>`. The tool description says it executes ordered browser operations,
but the callable schema does not reveal any valid step shape, so an agent cannot construct a batch
from the tool contract and must fall back to individual calls. This was observed directly from the
refreshed installed plugin's generated tool declaration during public-site form testing.
