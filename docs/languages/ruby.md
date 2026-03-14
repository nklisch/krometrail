---
title: Ruby
description: Debug Ruby programs with rdbg (Ruby debug gem).
---

# Ruby

**Debugger:** [rdbg](https://github.com/ruby/debug) (Ruby debug gem)
**Status:** Stable
**Ruby version:** 3.1+

## Prerequisites

The `debug` gem ships built-in with Ruby 3.1+. For older versions:

```bash
gem install debug
```

Verify: `rdbg --version`

## Quick Start

```bash
# Debug a script
krometrail launch "ruby app.rb" --break app.rb:42

# Debug with bundler
krometrail launch "bundle exec ruby app.rb" --break lib/order.rb:147

# Debug RSpec tests
krometrail launch "bundle exec rspec spec/order_spec.rb" --break lib/order.rb:147

# Debug Rails
krometrail launch "bundle exec rails server" --break app/controllers/orders_controller.rb:55
```

## How It Works

The adapter launches `rdbg --open --port PORT` to start a DAP server, then connects to it. Your program runs under `rdbg`, which handles breakpoints and step execution via the standard DAP protocol.

## Conditional Breakpoints

Ruby expressions work directly in conditions:

```bash
krometrail break "lib/order.rb:147 when discount < 0"
krometrail break "lib/loop.rb:25 when i == 99"
```

## Inspecting Ruby Objects

The viewport renders Ruby objects using `inspect`:

```
Locals:
  discount  = -149.97
  order     = #<Order:0x000... id=482, total=149.97, tier="gold">
  items     = Array(3): [#<Item price=49.99>, ...]
  result    = nil
```

## Tips

- Use `bundle exec` when your project uses Bundler to ensure the correct gem versions are loaded
- The `debug` gem supports Ruby 3.1+ natively; on older Rubies install the `debug` gem explicitly
- `debug_evaluate` accepts any valid Ruby expression including method calls and blocks
