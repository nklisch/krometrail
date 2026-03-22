# Node.js / TypeScript Debugging (js-debug)

## Prerequisites
js-debug is bundled — no install needed. Node.js must be in PATH.

```sh
krometrail doctor  # verify
```

## Launch

### Node.js
```
debug_launch(command: 'node app.js')
debug_launch(command: 'node --experimental-vm-modules server.js')
```

### TypeScript (via tsx or ts-node)
```
debug_launch(command: 'tsx app.ts')
debug_launch(command: 'ts-node app.ts')
debug_launch(command: 'node --loader ts-node/esm app.ts')
```

### Frameworks (auto-detected)

**Jest:**
```
debug_launch(command: 'jest tests/order.test.ts --no-coverage')
debug_launch(command: 'jest --testNamePattern "should apply discount"')
```

**Vitest:**
```
debug_launch(command: 'vitest run tests/order.test.ts')
```

**Next.js / Vite dev server:**
```
debug_launch(command: 'next dev')
debug_launch(command: 'vite')
```
Set breakpoints in route handlers or server components — client-side code requires `references/chrome.md`.

## Attach (--inspect)
Start the process:
```sh
node --inspect app.js              # port 9229
node --inspect=0.0.0.0:9229 app.js  # remote
node --inspect-brk app.js          # pause before first line
```
Then:
```
debug_attach(language: 'node', port: 9229)
```

## Exception Breakpoints
```
debug_set_exception_breakpoints(session_id: '...', filters: ['all'])      # all exceptions
debug_set_exception_breakpoints(session_id: '...', filters: ['uncaught']) # unhandled only
```

## Variable Scopes
- `local` — current function locals
- `closure` — closure variables (Node.js only)
- `global` — global scope

## Source Maps
TypeScript source maps are resolved automatically. Breakpoints should be set on `.ts` files, not `.js`.

If source maps aren't resolving, check that `sourceMap: true` is in `tsconfig.json`.

## Known Quirks
- ESM modules require `--experimental-vm-modules` for jest, or use `vitest` instead.
- Arrow functions in callbacks show as `<anonymous>` in the stack — use named functions for clarity.
- `console.log` output appears in `debug_output`, not the viewport.
- For browser-side JS in Next.js/Vite, use `chrome_start` + Chrome DevTools instead (see `references/chrome.md`).
