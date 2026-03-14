---
title: C / C++
description: Debug C and C++ programs with GDB 14+ or lldb-dap.
---

# C / C++

**Debugger:** GDB 14+ (`--interpreter=dap`) or lldb-dap
**Status:** Stable
**Transport:** stdin/stdout (not TCP)

## Prerequisites

::: code-group

```bash [GDB (Linux)]
# Ubuntu/Debian
sudo apt install gdb
gdb --version   # must be 14+

# Fedora/RHEL
sudo dnf install gdb
```

```bash [lldb-dap (macOS)]
# Via Xcode Command Line Tools
xcode-select --install

# Or via Homebrew
brew install llvm
lldb-dap --version
```

:::

GDB 14 added native DAP support via `--interpreter=dap`. Earlier versions are not supported.

## Quick Start

The adapter compiles source files automatically with debug symbols if you provide a `.c` or `.cpp` file:

```bash
# Compile and debug a C program
krometrail debug launch "gcc -g -o app app.c && ./app" --break app.c:42

# Or let the adapter compile:
krometrail debug launch "app.c" --break app.c:42

# C++ with g++
krometrail debug launch "g++ -g -o app app.cpp && ./app" --break app.cpp:42

# Debug an existing binary
krometrail debug launch "./app" --break app.c:42 --language cpp
```

## Conditional Breakpoints

C/C++ expressions:

```bash
krometrail debug break "order.c:147 when discount < 0"
krometrail debug break "loop.c:25 when i == 99"
krometrail debug break "api.c:30 when strcmp(method, \"POST\") == 0"
```

## Inspecting C/C++ Values

Structs and unions are shown with their fields:

```
Locals:
  order    = {id=482, total=149.97, tier=2}
  discount = -149.97
  items    = 0x... -> [{price=49.99, qty=3}, ...]
```

Pointers show the address and the dereferenced value. Use `debug_evaluate` with GDB expressions for deeper inspection:

```bash
krometrail debug eval "order->tier"
krometrail debug eval "*(double*)(&raw_value)"
krometrail debug eval "((Order*)ptr)->id"
```

## stdin/stdout Transport

Unlike other adapters, C/C++ uses stdin/stdout transport (GDB's DAP mode communicates via pipes, not TCP). This is handled transparently — no configuration needed.

## Tips

- Always compile with `-g` (debug info) and without `-O2`/`-O3` (optimization breaks debug info)
- For CMake projects: `cmake -DCMAKE_BUILD_TYPE=Debug ..` then debug the built binary
- Thread debugging works — `debug_threads` lists pthreads created by `pthread_create`
- Valgrind and AddressSanitizer can be combined: `krometrail debug launch "valgrind --vgdb=yes ./app"` for memory debugging

## GDB vs lldb-dap

`krometrail doctor` checks for both. GDB is the default on Linux; lldb-dap is preferred on macOS. Override with `--language` if needed:

```bash
krometrail debug launch "./app" --language cpp
```
