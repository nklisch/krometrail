---
title: Node.js / TypeScript
description: Debug Node.js and TypeScript with js-debug. Supports jest, mocha, and plain scripts.
---

# Node.js / TypeScript

**Debugger:** [js-debug](https://github.com/microsoft/vscode-js-debug) (Microsoft, built into VS Code)
**Status:** Stable
**Node.js version:** 18+

## Prerequisites

No installation required. The adapter downloads js-debug automatically on first use and caches it at `~/.krometrail/adapters/js-debug/`.

Verify: `node --version` (must be 18+)

## Quick Start

```bash
# Debug a Node.js script
krometrail launch "node index.js" --break index.js:42

# Debug TypeScript (ts-node)
krometrail launch "npx ts-node src/app.ts" --break src/app.ts:30

# Debug jest tests
krometrail launch "npx jest tests/order.test.js --no-coverage" \
	--break src/order.js:147

# Debug mocha tests
krometrail launch "npx mocha tests/**/*.test.js" --break src/api.js:83
```

## TypeScript

Source maps are enabled automatically. Set breakpoints against the TypeScript source files, not the compiled JavaScript:

```bash
krometrail break "src/order.ts:147 when discount < 0"
```

The adapter sets `outFiles` to map compiled output back to source locations. Ensure your `tsconfig.json` has `"sourceMap": true`.

## Framework Auto-detection

jest and mocha are auto-detected from the launch command. The adapter configures `--inspect-brk` correctly for each test runner and handles the test runner's argument parsing to avoid conflicts with the debugger flags.

## Conditional Breakpoints

JavaScript expressions work in conditions:

```bash
krometrail break "order.js:147 when discount < 0"
krometrail break "loop.js:25 when i === 99"
krometrail break "api.js:30 when req.method === 'POST'"
```

## Exception Breakpoints

```bash
krometrail break --exceptions uncaught
krometrail break --exceptions all       # including caught exceptions
```

## Evaluating Async Expressions

`debug_evaluate` supports `await` in the expression if you're stopped in an async context:

```bash
krometrail eval "await user.fetchProfile()"
```

## Tips

- The first launch may take a few seconds while js-debug is downloaded and extracted
- ESM (`.mjs`, `type: "module"`) is supported
- Worker threads appear as separate threads in `debug_threads`
- For Bun: the Bun adapter exists but is not currently supported (Bun uses WebKit JSC protocol, not V8 CDP). Use Node.js for debugging Bun-targeted code during development.
