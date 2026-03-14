---
title: Rust
description: Debug Rust programs with GDB or LLDB.
---

# Rust

**Debugger:** GDB / LLDB
**Status:** Stable

## Prerequisites

Install GDB or LLDB. One or both must be present in your PATH.

```bash
# Linux (GDB)
sudo apt install gdb       # Debian/Ubuntu
sudo dnf install gdb       # Fedora/RHEL

# macOS (LLDB, ships with Xcode Command Line Tools)
xcode-select --install

# Verify
gdb --version
lldb --version
```

The adapter uses LLDB when available, falling back to GDB.

## Quick Start

```bash
# Build first, then debug
cargo build
krometrail debug launch "cargo run" --break src/order.rs:147

# Debug tests
cargo test --no-run  # build test binary
krometrail debug launch "cargo test" --break src/order.rs:147

# Debug a specific test
krometrail debug launch "cargo test test_gold_discount -- --nocapture" \
	--break src/order.rs:147
```

## Conditional Breakpoints

Rust expressions:

```bash
krometrail debug break "src/order.rs:147 when discount < 0.0"
krometrail debug break "src/loop.rs:25 when i == 99"
```

## Inspecting Rust Types

The viewport renders Rust types using their `Debug` impl where available:

```
Locals:
  discount   = -149.97
  order      = Order { id: 482, total: 149.97, tier: Gold }
  items      = Vec(3): [Item { price: 49.99, qty: 3 }, ...]
  result     = Ok(ChargeResult { success: false, error: "card_declined" })
```

Enum variants show their discriminant and associated data. `Option` renders as `None` or `Some(value)`.

## Tips

- Build with debug symbols (`cargo build`, not `cargo build --release`) — release builds optimize out most debug information
- `cargo test` builds test binaries in `target/debug/deps/` — the adapter locates them automatically
- For workspace projects, specify `--bin` or `--test` to select the right binary
- Unsafe blocks and raw pointers can be inspected via `debug_evaluate` using GDB/LLDB expressions
