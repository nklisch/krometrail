# Go Debugging (Delve)

## Prerequisites
```sh
go install github.com/go-delve/delve/cmd/dlv@latest
krometrail doctor  # verify
```

## Launch
```
debug_launch(command: 'go run main.go')
debug_launch(command: 'go run ./cmd/server')
debug_launch(command: 'go test ./... -run TestOrder')
debug_launch(command: 'go test ./internal/order -run TestDiscount -v')
```

Language is auto-detected. Override with `language: 'go'` if needed.

## Attach (by PID)
```sh
# Find PID
ps aux | grep myapp
```
```
debug_attach(language: 'go', pid: 12345)
```

## Exception Breakpoints (Panics)
```
debug_set_exception_breakpoints(session_id: '...', filters: ['panic'])
```

## Variable Scopes
- `local` — current function locals (default)
- `global` — package-level globals

## Evaluating Expressions
Go's Delve supports a subset of Go expressions:
```
debug_evaluate(session_id: '...', expression: 'order.Total')
debug_evaluate(session_id: '...', expression: 'len(cart.Items)')
debug_evaluate(session_id: '...', expression: '*ptr')           # dereference pointer
```
- Complex expressions with function calls may not work in all contexts.
- Interface values show the concrete type and value.
- Maps and slices show their contents up to `max_depth`.

## Known Quirks
- `go run` compiles a temporary binary — breakpoints must use the source file path, not the binary.
- CGo code cannot be debugged with Delve.
- Goroutines all appear in `debug_threads` — use `--session` carefully if multiple goroutines are stopped.
- Inlined functions may not be steppable — build with `-gcflags=all=-N -l` to disable optimizations:
  ```
  debug_launch(command: 'go run -gcflags=all=-N -l main.go')
  ```
