---
id: idea-mcp-locator-ergonomics
created: 2026-08-15
updated: 2026-08-15
tags: [agent-ux]
---

# Lenient & Flattened Locators for Agent Interaction Tools

## The Finding
During multi-site dogfooding across Wikipedia, TodoMVC, and GitHub, the primary source of tool argument validation failures (`invalid_input`) was the deep nesting required by `locator` parameters in `click`, `fill`, and `hover`.

Currently, selecting by CSS requires:
```json
{
  "locator": {
    "kind": "element",
    "value": {
      "kind": "css_selector",
      "value": "button.submit"
    }
  }
}
```

And by coordinate:
```json
{
  "locator": {
    "kind": "coordinate",
    "value": {
      "point": { "x": 100.0, "y": 200.0 },
      "space": "viewport_css"
    }
  }
}
```

LLM agents frequently emit flat `{ "locator": "button.submit" }`, `{ "selector": "button.submit" }`, or `{ "locator": { "kind": "css_selector", "value": "button.submit" } }`. The strict double-nesting causes repeated tool rejection roundtrips.

## Fix Direction
Enhance `LocatorWire` in `crates/krometrail-mcp/src/schema.rs` with untagged or flattened serde aliases:
1. **String shorthand**: If `locator` is a plain string, parse it as a CSS selector (`kind: "element", value: { kind: "css_selector", value: str }`).
2. **Flat object shorthand**: If `locator` is an object with `selector` (or `css`), coerce to `css_selector`. If it has `x` and `y`, coerce to `coordinate` with `viewport_css` default.
3. Preserve full strict AST serialization on the internal core boundary while accepting ergonomic shorthands at the MCP gateway.
