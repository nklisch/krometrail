---
title: Replay Context
description: Your agent can generate reproduction steps or test scaffolds from your recorded session.
---

# Replay Context

Your agent can generate reproduction steps or test scaffolds from your recorded session — a minimal sequence of actions that reproduces an observed bug, or a failing test that captures the regression.

## Output Formats

Your agent can produce three kinds of output from a session:

**Steps** — a numbered list of human-readable actions that reproduce the bug:

```
1. Navigate to http://localhost:3000/checkout
2. Fill "email" field with "user@example.com"
3. Click "Place Order" button
4. [Network] POST /api/orders → 500 Internal Server Error
5. [Console] Error: Failed to process payment: card_declined
```

**Playwright scaffold** — a TypeScript test that automates the reproduction:

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

**Cypress scaffold** — the same reproduction in Cypress test syntax.

## Use Cases

- **Bug reports** — convert a recorded session into a reproducible sequence for filing issues with your team
- **Regression tests** — generate a failing test before fixing the bug, then make it pass
- **Documentation** — create human-readable steps for design or QA review
- **Agent debugging** — your agent uses replay context to understand the exact sequence of events that led to an error before diving into source code

## Tips

- **Use markers to scope the output** — if you marked "page loaded" and "error appeared", your agent can generate steps for just that portion of the session, not the entire recording
- **Expect to adjust generated tests** — test scaffolds need manual tweaks for dynamic values like auth tokens, generated IDs, and timestamps
- **The steps format is often enough** — your agent can form a debugging hypothesis from the human-readable steps without needing to read through raw events
