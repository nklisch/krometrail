# JavaScript / TypeScript Debugging

## Prerequisites

- `node` 18+ required
- js-debug adapter (auto-downloaded on first use)
- Verify: `node --version`

## Launch examples

```
# Node.js script
debug_launch({ command: "node app.js" })

# TypeScript (via tsx/ts-node)
debug_launch({ command: "npx tsx script.ts" })

# Jest tests
debug_launch({ command: "npx jest tests/cart.test.js" })

# Mocha tests
debug_launch({ command: "npx mocha tests/" })

# With arguments
debug_launch({ command: "node server.js --port 3000" })
```

## Attach to running process

Start Node.js with the inspector enabled:

```bash
node --inspect app.js          # default port 9229
node --inspect=9230 app.js     # custom port
```

Then:
```
debug_attach({ language: "javascript", port: 9229 })
```

## Exception breakpoints

| Filter | Breaks on |
|--------|-----------|
| `all` | All exceptions (including caught) |
| `uncaught` | Only unhandled exceptions |

## Conditional breakpoints

JavaScript expressions:

```
debug_set_breakpoints({
  session_id: "...",
  file: "cart.js",
  breakpoints: [
    { line: 15, condition: "items.length === 0" },
    { line: 30, condition: "total < 0" }
  ]
})
```

## Tips

- Source maps are supported — set breakpoints in `.ts` files when using TypeScript
- The adapter uses a two-session model internally (parent + child) — this is transparent
- `--inspect` flags in the command are automatically handled
