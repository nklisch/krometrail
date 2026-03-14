---
title: Kotlin
description: Debug Kotlin programs with JDWP.
---

# Kotlin

**Debugger:** JDWP (Java Debug Wire Protocol, built into the JVM)
**Status:** Stable
**JDK version:** 11+

## Prerequisites

JDWP is built into the JVM — no separate debugger installation needed.

Verify: `java --version` (must be 11+)

## Quick Start

```bash
# Debug a compiled Kotlin program
krometrail launch "java -jar app.jar" --break src/main/kotlin/OrderService.kt:147

# Debug with Gradle
krometrail launch "./gradlew run" --break src/main/kotlin/OrderService.kt:147

# Debug Kotlin tests
krometrail launch "./gradlew test" --break src/main/kotlin/OrderService.kt:147

# Debug a specific test
krometrail launch "./gradlew test --tests OrderServiceTest.testGoldDiscount" \
	--break src/main/kotlin/OrderService.kt:147
```

## How It Works

The adapter starts the JVM with JDWP agent flags (`-agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=PORT`) and connects via the DAP-to-JDWP bridge. Kotlin source maps are used to resolve breakpoint locations from `.kt` source files to JVM bytecode.

## Conditional Breakpoints

Java/JVM expressions work in conditions (Kotlin syntax is not fully supported in JDWP expressions):

```bash
krometrail break "src/main/kotlin/OrderService.kt:147 when discount < 0"
```

## Inspecting Kotlin Objects

The viewport renders Kotlin/JVM objects using their field values:

```
Locals:
  discount  = -149.97
  order     = Order@482 {id=482, total=149.97, tier=GOLD}
  items     = ArrayList(3): [Item@12, Item@13, Item@14]
  result    = null
```

## Tips

- Compile with debug info included — Gradle does this by default for non-release builds
- Coroutine debugging: use `debug_threads` to inspect coroutine threads; the adapter exposes coroutine state via JDWP
- Data classes render all their component properties in the viewport
- For Android projects, device/emulator JDWP debugging requires the `adb` forward command first
