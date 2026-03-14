---
title: Multi-threaded Debugging
description: List threads and goroutines, select a thread, and step within it.
---

# Multi-threaded Debugging

When debugging multi-threaded programs or Go goroutines, Krometrail exposes thread listing and selection so you can inspect any thread's state independently.

## Listing Threads

::: code-group

```bash [CLI]
krometrail debug threads
```

```json [MCP: debug_threads]
{ "session_id": "..." }
```

:::

Returns all threads (or goroutines in Go), including:

- Thread/goroutine ID
- Name (if available)
- Current location (file:line:function)
- Status: `stopped`, `running`, or `blocked`

Example output for a Go program:

```
Threads (4):
  * 1  goroutine 1     main.processOrders   order.go:147    stopped (breakpoint)
    2  goroutine 6     net/http.serve       server.go:3086  running
    3  goroutine 7     runtime.gcBgMark     mgc.go:2431     running
    4  goroutine 8     main.backgroundJob   jobs.go:52      blocked (chan receive)
```

The `*` marks the currently selected thread.

## Selecting a Thread

After listing threads, switch context to a specific thread:

::: code-group

```bash [CLI]
krometrail debug threads --select 4
```

```json [MCP: debug_threads]
{ "session_id": "...", "select_thread_id": 4 }
```

:::

Returns the viewport for the selected thread — source, locals, and call stack at that thread's current location.

Subsequent `debug_step`, `debug_continue`, and state inspection commands operate on the selected thread until you switch again.

## Per-Thread Stepping

Once a thread is selected, all execution control commands apply to that thread:

```bash
# Select a thread
krometrail debug threads --select 4

# Step within that thread
krometrail debug step over

# The other threads remain in their current state
```

## Language-Specific Notes

**Python** — threads correspond to OS threads created by `threading.Thread`. The GIL limits true parallelism, but the debugger shows all threads.

**Node.js** — single-threaded event loop. Worker threads (from `worker_threads` module) appear as separate threads when active.

**Go** — goroutines, not OS threads. Delve maps goroutines to the `debug_threads` concept. Goroutine count can be in the hundreds for typical Go programs; filter to `stopped` status to find interesting ones.

**Java** — OS threads. All threads created by the JVM (including GC threads) are listed. Filter by status or name to focus on application threads.

**Rust** — OS threads. Multi-threaded programs using `std::thread::spawn` expose each thread.

**C/C++** — OS threads via GDB/lldb. Signal handling threads may appear.

## Tips

- **Filter by status** — most threads are `running` or `blocked` in background work. Focus on `stopped` threads when a breakpoint fires.
- **Goroutines vs. threads** — Go programs may have dozens to thousands of goroutines. The `debug_threads` output limits to a reasonable count; use the `filter` parameter to narrow by name or status.
- **Thread-local state** — each thread has its own call stack and local variables. Switch threads with `debug_threads` to inspect state from a different execution context.
