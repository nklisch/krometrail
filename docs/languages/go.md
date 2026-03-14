---
title: Go
description: Debug Go programs and tests with Delve (dlv).
---

# Go

**Debugger:** [Delve](https://github.com/go-delve/delve) (`dlv`)
**Status:** Stable
**Go version:** 1.18+

## Prerequisites

```bash
go install github.com/go-delve/delve/cmd/dlv@latest
```

Verify: `dlv version`

## Quick Start

```bash
# Debug a Go program
krometrail debug launch "go run main.go" --break main.go:42

# Debug Go tests
krometrail debug launch "go test ./..." --break service/order.go:147

# Debug a specific test function
krometrail debug launch "go test -run TestGoldDiscount ./service/..." \
	--break service/order.go:147

# Debug with race detector
krometrail debug launch "go test -race ./..." --break service/order.go:147
```

## Test Framework Auto-detection

`go test` commands are auto-detected. The adapter converts `go test` into the equivalent `dlv test` invocation so Delve can instrument the test binary directly.

## Goroutine Debugging

Go programs often have many goroutines. Use `debug_threads` to list and select them:

```bash
# List all goroutines
krometrail debug threads

# Select a specific goroutine
krometrail debug threads --select 6

# Then step within that goroutine
krometrail debug step over
```

Goroutine names (set via `runtime.SetGoroutineLabels`) appear in the thread list.

## Conditional Breakpoints

Go expressions:

```bash
krometrail debug break "order.go:147 when discount < 0"
krometrail debug break "loop.go:25 when i == 99"
krometrail debug break "api.go:30 when req.Method == \"POST\""
```

## Tips

- The adapter uses `dlv dap --listen :PORT` — the DAP transport, not the legacy CLI transport
- Interface values are shown with their concrete type: `<io.Reader: *os.File: "stdout">`
- Pointer types show the dereferenced value: `*Order: {id: 482, total: 149.97}`
- For debugging tests that compile slowly, run `go test -c -o test_binary ./...` first, then `krometrail debug launch "./test_binary -test.run TestName"`
- Set `CGO_ENABLED=0` if your code has CGo and breakpoints aren't hitting in C portions

## Troubleshooting

**Breakpoints adjusted to different line:**
- Delve may move breakpoints to the nearest executable line. This is normal — the confirmed line is reported back after `debug_set_breakpoints`.

**`dlv: command not found`:**
- Ensure `$(go env GOPATH)/bin` is in your PATH
- Re-run `go install github.com/go-delve/delve/cmd/dlv@latest`
