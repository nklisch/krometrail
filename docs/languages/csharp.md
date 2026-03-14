---
title: C#
description: Debug C# and .NET programs with netcoredbg.
---

# C# (.NET)

**Debugger:** [netcoredbg](https://github.com/Samsung/netcoredbg) (Samsung, MIT license)
**Status:** Stable
**.NET version:** .NET Core 3.1+ / .NET 5+

## Prerequisites

Download and install netcoredbg from the [releases page](https://github.com/Samsung/netcoredbg/releases) and place the `netcoredbg` binary in your PATH.

Verify: `netcoredbg --version`

## Quick Start

```bash
# Debug a .NET program
krometrail debug launch "dotnet run" --break Program.cs:42

# Debug with a specific project
krometrail debug launch "dotnet run --project src/MyApp" --break src/MyApp/OrderService.cs:147

# Debug tests
krometrail debug launch "dotnet test" --break src/MyApp/OrderService.cs:147

# Debug a specific test
krometrail debug launch "dotnet test --filter OrderServiceTests.TestGoldDiscount" \
	--break src/MyApp/OrderService.cs:147
```

## Conditional Breakpoints

C# expressions work directly in conditions:

```bash
krometrail debug break "OrderService.cs:147 when discount < 0"
krometrail debug break "OrderService.cs:147 when user.Tier == \"Gold\""
```

## Inspecting .NET Objects

The viewport renders .NET objects using their property values:

```
Locals:
  discount  = -149.97
  order     = Order { Id=482, Total=149.97, Tier=Gold }
  items     = List(3): [Item { Price=49.99, Qty=3 }, ...]
  result    = null
```

## Tips

- Build with debug configuration (`dotnet build` without `--configuration Release`) to include debug symbols
- For ASP.NET Core: launch with `dotnet run` — the adapter handles attaching before requests are served
- `debug_evaluate` accepts C# expressions including LINQ queries
- netcoredbg is open-source (MIT) and does not require a Visual Studio license
