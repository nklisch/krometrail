# Agent Lens — Testing Strategy

---

## Philosophy

Agent Lens has a unique property: **the entire dependency chain is local**. The debugee is a script we create, the debugger is a local process, and DAP is a local socket. There is no network, no external API, no cloud service. We own every piece of the stack from the MCP/CLI entry point down to the debugee process.

This means **e2e and integration tests should never mock anything**.

> **Prior art note:** debugger-mcp validates their implementation with integration tests against *real Claude Code and Codex agents* — 5 languages x 2 agents = 10 test matrices, all passing. mcp-debugger claims 1000+ tests using vitest with Docker-based CI. Both projects confirm that real-debugger testing is viable and essential. See [PRIOR_ART.md](PRIOR_ART.md). We set up real code, launch real debuggers, send real DAP messages, and assert on real viewport output. Mocking would only hide the bugs we're trying to catch — protocol mismatches, adapter quirks, debugger version differences.

Mocks are reserved for unit tests of pure logic (viewport rendering, value formatting, session state machines) where isolating a unit genuinely helps.

---

## Test Tiers

### Unit Tests

**What:** Pure functions and isolated modules. Viewport rendering, value formatting, DAP message framing, CLI argument parsing, session state transitions.

**How:** Standard vitest. Fast, no external processes. These run in milliseconds.

**Location:** `tests/unit/` — mirrors `src/` structure.

```
tests/unit/
├── core/
│   ├── viewport.test.ts
│   ├── value-renderer.test.ts
│   ├── session-manager.test.ts
│   └── dap-client.test.ts
├── cli/
│   └── arg-parser.test.ts
└── mcp/
    └── tool-handlers.test.ts
```

**Mocking:** Allowed here. Mock DAP connections to test the session manager in isolation, mock filesystem to test source reading, etc.

### Integration Tests

**What:** Component boundaries. DAP client talking to a real debugger. Adapter launching a real process. MCP tool handler calling through to a real session.

**How:** Launch real debugger processes. Set real breakpoints. Step through real code. Assert on real DAP responses. No mocks.

**Location:** `tests/integration/`

```
tests/integration/
├── adapters/
│   ├── python.test.ts      # debugpy
│   ├── node.test.ts         # node --inspect
│   └── go.test.ts           # dlv
├── dap-client.test.ts       # Real DAP session against debugpy
└── session-lifecycle.test.ts # Full session: launch → break → step → stop
```

**Fixtures:** Real programs in `tests/fixtures/` designed to exercise specific debugging scenarios.

### E2E Tests

**What:** Full stack from the agent-facing interface (MCP tool call or CLI command) through to debugee observation. Tests the exact experience an agent would have.

**How:** Call MCP tools or invoke CLI commands. Assert on viewport output text. Verify the complete chain works end-to-end.

**Location:** `tests/e2e/`

```
tests/e2e/
├── mcp/
│   ├── launch-and-break.test.ts
│   ├── step-and-inspect.test.ts
│   ├── conditional-breakpoints.test.ts
│   └── session-limits.test.ts
├── cli/
│   ├── launch-and-break.test.ts
│   ├── step-and-inspect.test.ts
│   └── daemon-lifecycle.test.ts
└── scenarios/
    ├── discount-bug.test.ts       # The canonical example from the design doc
    ├── exception-tracing.test.ts
    └── multi-breakpoint.test.ts
```

---

## Test Fixtures

Real programs of varying complexity that exercise specific debugging scenarios:

```
tests/fixtures/
├── python/
│   ├── simple-loop.py           # Basic stepping, variable inspection
│   ├── function-calls.py        # Call stack, step into/out
│   ├── exception-raising.py     # Exception breakpoints
│   ├── discount-bug.py          # The canonical design doc example
│   └── class-state.py           # Object inspection, nested attributes
├── node/
│   ├── simple-loop.js
│   ├── async-await.js           # Async debugging
│   └── express-handler.js       # Framework debugging
└── go/
    ├── simple-loop.go
    └── goroutine.go             # Multi-threaded debugging
```

Fixtures are deliberately simple. Each exercises one or two debugging scenarios. Complex real-world code is not the goal — reliable, deterministic assertion targets are.

---

## Debugger Setup

### Local Development

`scripts/setup-test-deps.sh` — checks for and installs required debuggers:

```bash
# Python: debugpy
pip install debugpy

# Node.js: built-in, just need node
node --version

# Go: delve
go install github.com/go-delve/delve/cmd/dlv@latest
```

The script is idempotent and reports what's already installed vs what it installed. Tests that require a missing debugger are automatically skipped (vitest `describe.skipIf`).

### CI (Docker)

`Dockerfile.test` — all debuggers pre-installed for reproducible CI:

```dockerfile
FROM oven/bun:latest

# Python + debugpy
RUN apt-get update && apt-get install -y python3 python3-pip
RUN pip3 install debugpy

# Node.js (for node adapter tests)
RUN apt-get install -y nodejs

# Go + Delve (for go adapter tests)
RUN apt-get install -y golang
RUN go install github.com/go-delve/delve/cmd/dlv@latest

WORKDIR /app
COPY . .
RUN bun install
```

---

## Running Tests

```bash
# All tests
bun run test

# Unit tests only (fast)
bun run test:unit

# Integration tests (needs debuggers installed)
bun run test:integration

# E2E tests (needs debuggers installed)
bun run test:e2e

# Specific adapter
bun run test tests/integration/adapters/python.test.ts

# Watch mode (unit tests)
bun run test:unit --watch
```

---

## Vitest Configuration

Tests are organized by tier using vitest workspaces or path-based filtering:

```typescript
// vitest.config.ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    testTimeout: 30_000,       // Debugger launches can be slow
    hookTimeout: 15_000,
  },
});
```

Package.json scripts:

```json
{
  "scripts": {
    "test": "vitest run",
    "test:unit": "vitest run tests/unit",
    "test:integration": "vitest run tests/integration",
    "test:e2e": "vitest run tests/e2e",
    "test:watch": "vitest watch tests/unit"
  }
}
```

---

## Assertions

E2E tests assert on the actual viewport text output. This is intentional — the viewport is the contract with the agent. If the format changes, tests should break:

```typescript
// Example e2e test
test("breakpoint shows locals with correct values", async () => {
  const session = await launchDebug("python", "tests/fixtures/python/discount-bug.py", {
    breakpoints: [{ file: "discount-bug.py", line: 12 }],
  });

  const viewport = await session.continue();

  expect(viewport).toContain("STOPPED at discount-bug.py:12");
  expect(viewport).toContain("discount  = -149.97");
  expect(viewport).toMatch(/subtotal\s+=\s+149\.97/);

  await session.stop();
});
```
