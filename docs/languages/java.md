---
title: Java
description: Debug Java programs with JDWP.
---

# Java

**Debugger:** JDWP (Java Debug Wire Protocol, built into the JVM)
**Status:** Stable
**JDK version:** 11+

## Prerequisites

JDWP is built into the JVM — no separate debugger installation needed.

Verify: `java --version` (must be 11+)

## Quick Start

```bash
# Debug a plain Java program
krometrail launch "java -cp target/classes com.example.Main" \
	--break src/main/java/com/example/OrderService.java:147

# Debug with Maven
krometrail launch "mvn exec:java -Dexec.mainClass=com.example.Main" \
	--break src/main/java/com/example/OrderService.java:147

# Debug JUnit tests
krometrail launch "mvn test -Dtest=OrderServiceTest" \
	--break src/main/java/com/example/OrderService.java:147
```

## How It Works

The adapter starts the JVM with JDWP agent flags (`-agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=PORT`) injected automatically. No changes to your code or build config are needed.

## Conditional Breakpoints

Java expressions:

```bash
krometrail break "OrderService.java:147 when discount < 0"
krometrail break "OrderService.java:147 when user.getTier().equals(\"GOLD\")"
```

## Thread Debugging

Java programs typically have many threads (GC, finalizer, etc.). Use `debug_threads` to navigate:

```bash
krometrail threads
# Focus on application threads — filter out GC/finalizer by name
```

## Inspecting Java Objects

The viewport renders Java objects using their field values:

```
Locals:
  order    = Order@482 {id=482, total=149.97, tier=GOLD}
  discount = -149.97
  items    = ArrayList(3): [Item@12, Item@13, Item@14]
```

Use `debug_evaluate` for deeper inspection:

```bash
krometrail eval "order.getDiscount().getClass().getName()"
krometrail eval "items.stream().mapToDouble(Item::getPrice).sum()"
```

## Tips

- Compile with `-g` flag for full debug information (`javac -g` or set in build tool)
- For Spring Boot: launch with `./mvnw spring-boot:run` or the equivalent Gradle command
- Inner classes appear as `OuterClass$InnerClass` in breakpoint paths
- Lambda breakpoints work — set the breakpoint on the line containing the lambda expression
