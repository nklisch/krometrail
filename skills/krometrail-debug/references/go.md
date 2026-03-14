# Go Debugging

## Prerequisites

- `dlv` (Delve) available in PATH, `$GOPATH/bin`, or `~/go/bin`
- Install: `go install github.com/go-delve/delve/cmd/dlv@latest`
- Verify: `dlv version`

## Launch examples

```
# go run (debug mode)
debug_launch({ command: "go run main.go" })

# go test (test mode)
debug_launch({ command: "go test ./..." })

# go test specific package
debug_launch({ command: "go test ./pkg/cart/ -run TestDiscount" })

# Pre-built binary (exec mode)
debug_launch({ command: "./myapp" })

# With build flags
debug_launch({ command: "go run -race main.go" })
```

## Attach to running process

Attach by PID:

```
debug_attach({ language: "go", pid: 12345 })
```

## Exception breakpoints

| Filter | Breaks on |
|--------|-----------|
| `panic` | Runtime panics |

## Conditional breakpoints

Go expressions (Delve syntax):

```
debug_set_breakpoints({
  session_id: "...",
  file: "cart.go",
  breakpoints: [
    { line: 42, condition: "i > 50" },
    { line: 55, condition: "err != nil" }
  ]
})
```

## Tips

- Build flags (e.g., `-race`, `-tags`) are automatically extracted from the command
- Goroutines appear as threads — use `debug_threads` to list them
- `go test` mode is auto-detected from the command
- Delve path is auto-discovered in `$GOPATH/bin` or `~/go/bin` if not in PATH
