---
title: Swift
description: Debug Swift programs with lldb-dap.
---

# Swift

**Debugger:** lldb (via lldb-dap, ships with the Swift toolchain)
**Status:** Stable
**Swift version:** 5.5+

## Prerequisites

`lldb-dap` ships with the Swift toolchain and Xcode Command Line Tools. No separate installation needed.

```bash
# Install Xcode Command Line Tools (macOS)
xcode-select --install

# Or install the Swift toolchain (Linux)
# https://www.swift.org/download/
```

Verify: `lldb-dap --version`

## Quick Start

```bash
# Debug a Swift package
krometrail launch "swift run" --break Sources/App/main.swift:42

# Debug a specific target
krometrail launch "swift run MyApp" --break Sources/MyApp/OrderService.swift:147

# Debug tests
krometrail launch "swift test" --break Sources/App/OrderService.swift:147

# Debug a specific test
krometrail launch "swift test --filter OrderServiceTests/testGoldDiscount" \
	--break Sources/App/OrderService.swift:147
```

## Conditional Breakpoints

Swift expressions work directly in conditions:

```bash
krometrail break "Sources/App/OrderService.swift:147 when discount < 0.0"
krometrail break "Sources/App/OrderService.swift:25 when i == 99"
```

## Inspecting Swift Types

The viewport renders Swift types using LLDB's type system:

```
Locals:
  discount  = -149.97
  order     = Order(id: 482, total: 149.97, tier: .gold)
  items     = Array<Item>(3 elements)
  result    = Optional<ChargeResult>.none
```

Enum cases show their associated values. Optionals render as `.none` or `.some(value)`.

## Tips

- Build with debug configuration (`swift build` without `-c release`) to include DWARF debug info
- `debug_evaluate` accepts Swift expressions including computed properties and closures
- On macOS, `lldb-dap` is typically at `/usr/bin/lldb-dap` or inside the Xcode bundle
- For iOS/watchOS/tvOS targets, device debugging is not yet supported — simulator only
