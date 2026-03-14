---
title: session_replay_context
description: Generate reproduction steps and Playwright/Cypress test scaffolds from a recorded session.
---

# session_replay_context

Generate reproduction steps or test scaffolds from a recorded session. Use this to produce a minimal sequence of actions that reproduces an observed bug, or to create a failing test that captures the regression.

## Usage

::: code-group

```bash [CLI]
# Human-readable reproduction steps
krometrail session replay-context <session-id>

# Playwright test scaffold
krometrail session replay-context <session-id> --format playwright

# Cypress test scaffold
krometrail session replay-context <session-id> --format cypress

# Scope to a time window
krometrail session replay-context <session-id> --format playwright \
	--from-marker "page loaded" --to-marker "error appeared"
```

```json [MCP: session_replay_context]
// Playwright scaffold
{
	"session_id": "abc123",
	"format": "playwright"
}

// Scoped to markers
{
	"session_id": "abc123",
	"format": "playwright",
	"from_marker": "page loaded",
	"to_marker": "error appeared"
}
```

:::

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `session_id` | string | The recording session |
| `format` | string | Output format: `"steps"` (default), `"playwright"`, `"cypress"` |
| `from_marker` | string | Start scope at this marker |
| `to_marker` | string | End scope at this marker |
| `from_ms` | number | Start scope at timestamp (ms from session start) |
| `to_ms` | number | End scope at timestamp (ms from session start) |

## Output Formats

**`steps`** — numbered list of human-readable actions:
```
1. Navigate to http://localhost:3000/checkout
2. Fill "email" field with "user@example.com"
3. Click "Place Order" button
4. [Network] POST /api/orders → 500 Internal Server Error
5. [Console] Error: Failed to process payment: card_declined
```

**`playwright`** — a TypeScript test scaffold:
```typescript
import { test, expect } from "@playwright/test";

test("reproduce: order submission fails with card_declined", async ({ page }) => {
	await page.goto("http://localhost:3000/checkout");
	await page.fill('[name="email"]', "user@example.com");
	await page.click('button:has-text("Place Order")');

	// Assert the 500 response (intercept network)
	// TODO: Add assertions for the error state
});
```

**`cypress`** — a Cypress test scaffold in similar form.

## Use Cases

- **Bug reports** — convert a recorded session into a reproducible sequence for filing issues
- **Regression tests** — generate a failing test before fixing the bug, then make it pass
- **Documentation** — create human-readable steps for design or QA review
- **Agent debugging** — the agent uses `session_replay_context` to understand the exact sequence of events that led to an error before setting breakpoints

## Tips

- Use markers to scope the replay to just the relevant portion of the session
- Generated test scaffolds need manual adjustment for dynamic values (tokens, IDs, timestamps)
- The `steps` format is often enough context for an agent to form a debugging hypothesis without reading raw events
