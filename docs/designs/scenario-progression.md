# Scenario Progression Design

Escalating debugging scenarios for the agent test harness. Each level requires deeper investigation and more sophisticated use of debugging tools.

Focus: **Python first**, then replicate patterns for other languages.

## Difficulty Levels

### Level 1 — Read the Code
Bug is visible in source if you look carefully. Agent could fix it without running anything, but debugging confirms the fix.

### Level 2 — Run and Trace
Bug isn't obvious from reading. Agent needs to run the test, read the error, and trace the logic path to find the wrong value.

### Level 3 — Inspect Runtime State
Bug depends on runtime state that isn't obvious from the code. Shared mutation, incorrect initialization, or values that look right individually but interact wrong.

### Level 4 — Multi-Component Interaction
Bug spans multiple functions or modules. The error manifests far from its root cause. Agent must trace data flow across boundaries.

### Level 5 — Subtle and Adversarial
Edge cases, floating point issues, ordering assumptions, or bugs that only manifest under specific input conditions. The code looks completely correct on first read.

---

## Python Scenarios

### 1. `python-wrong-constant` (Level 1)
**Existing: `python-discount-bug`**

A dictionary maps tier names to discount rates. Gold is `1.0` instead of `0.1`. Visible in the code, one character fix.

**What it tests:** Can the agent read code, find an obvious wrong value, fix it.

---

### 2. `python-off-by-one` (Level 1)
**Existing: `python-off-by-one`**

`range(len(items) - 1)` skips the last item. Classic off-by-one.

**What it tests:** Can the agent trace a loop boundary issue.

---

### 3. `python-wrong-merge` (Level 2)
**Bug:** A function merges two sorted lists but uses `>=` instead of `>` in the comparison, causing duplicate elements to appear in the wrong order (unstable merge). The test checks that merge preserves insertion order for equal elements.

```python
def merge_sorted(a, b, key=None):
    result = []
    i = j = 0
    while i < len(a) and j < len(b):
        if key(a[i]) >= key(b[j]):  # BUG: should be > for stable merge
            result.append(b[j])
            j += 1
        else:
            result.append(a[i])
            i += 1
    result.extend(a[i:])
    result.extend(b[j:])
    return result
```

**What it tests:** Agent must understand stable sort semantics. The output looks "mostly right" — elements are sorted, but equal-key elements from `a` appear after those from `b`.

---

### 4. `python-shadow-variable` (Level 2)
**Bug:** A variable is reused across two loops. The second loop expects it to be reset but it retains the value from the first loop's last iteration.

```python
def process_orders(orders):
    # First pass: validate
    total = 0
    for order in orders:
        total = order["quantity"] * order["price"]
        if total < 0:
            raise ValueError("negative total")

    # Second pass: accumulate
    # BUG: `total` is still the last order's value, not 0
    for order in orders:
        total += order["quantity"] * order["price"]

    return total
```

**What it tests:** Agent needs to trace variable state across loop boundaries. The bug is that `total` isn't reset to 0 before the second loop — it carries the last iteration's value from the first loop plus all values from the second.

---

### 5. `python-default-mutable` (Level 3)
**Bug:** A function uses a mutable default argument that persists across calls.

```python
def add_item(name, price, cart=None):
    if cart is None:
        cart = {"items": [], "total": 0.0}
    cart["items"].append({"name": name, "price": price})
    cart["total"] += price
    return cart

def create_carts(item_lists):
    carts = []
    for items in item_lists:
        cart = {}
        for name, price in items:
            cart = add_item(name, price, cart if cart.get("items") else None)
        carts.append(cart)
    return carts
```

Wait — the classic version is simpler. A function that uses `def f(items=[])` and accumulates state across calls.

```python
def register_sale(item, price, ledger=[]):
    """Record a sale and return the current ledger."""
    ledger.append({"item": item, "price": price})
    return ledger

def daily_report(sales_by_day):
    """Generate a report for each day's sales."""
    reports = []
    for day_sales in sales_by_day:
        ledger = register_sale(day_sales[0][0], day_sales[0][1])
        for item, price in day_sales[1:]:
            register_sale(item, price, ledger)
        reports.append({"count": len(ledger), "total": sum(s["price"] for s in ledger)})
    return reports
```

**What it tests:** The mutable default `ledger=[]` leaks state between days. Day 2's ledger contains Day 1's items. Agent must recognize the Python-specific footgun.

---

### 6. `python-closure-late-binding` (Level 3)
**Bug:** Closures in a loop capture the variable by reference, not by value.

```python
def make_validators(ranges):
    """Create validator functions for numeric ranges."""
    validators = []
    for name, low, high in ranges:
        def validate(value):
            return low <= value <= high
        validators.append((name, validate))
    return validators

# All validators use the last range's low/high
```

**What it tests:** Agent must understand Python closure scoping. All validators reference the same `low` and `high` variables, which hold the last loop iteration's values. Fix: use default args `def validate(value, low=low, high=high)`.

---

### 7. `python-dict-iteration-mutation` (Level 4)
**Bug:** A function modifies a dict while iterating a view derived from it, but the mutation doesn't cause a RuntimeError — it silently produces wrong results because the iteration completes before the mutation matters.

```python
def apply_promotions(catalog, promotions):
    """Apply promotional prices to catalog items."""
    updated = 0
    for sku, promo_price in promotions.items():
        if sku in catalog:
            old_price = catalog[sku]["price"]
            catalog[sku]["price"] = promo_price
            catalog[sku]["savings"] = old_price - promo_price
            updated += 1

    # BUG: calculate stats from catalog, but some prices were
    # already overwritten above. "average original price" is wrong.
    original_prices = [item["price"] for item in catalog.values()]
    avg_original = sum(original_prices) / len(original_prices)

    return {"updated": updated, "avg_original_price": avg_original}
```

**What it tests:** The mutation and the "read" happen in the same function. Agent must realize the list comprehension sees already-mutated prices for promoted items. The fix is to capture original prices before applying promotions.

---

### 8. `python-float-accumulation` (Level 5)
**Bug:** Floating point accumulation error causes an equality check to fail.

```python
def split_bill(total, num_people, tip_pct=0.18):
    """Split a restaurant bill evenly."""
    tip = total * tip_pct
    bill_with_tip = total + tip
    per_person = bill_with_tip / num_people

    # Verify the split adds up
    shares = [per_person] * num_people
    total_shares = sum(shares)

    if total_shares != bill_with_tip:  # BUG: float comparison
        # "Correction" that actually makes it worse
        shares[-1] += bill_with_tip - total_shares

    return {
        "per_person": round(per_person, 2),
        "shares": [round(s, 2) for s in shares],
        "total_with_tip": round(bill_with_tip, 2),
        "total_shares": round(sum(round(s, 2) for s in shares), 2),
    }
```

**What it tests:** `per_person * num_people != bill_with_tip` due to float precision. The "correction" adds a tiny epsilon to the last share. After rounding, the shares don't sum to the total. Agent must understand float arithmetic and fix the comparison with `math.isclose` or restructure the rounding.

---

### 9. `python-generator-exhaustion` (Level 4)
**Bug:** A generator is consumed on first use, then silently yields nothing on second use.

```python
def load_transactions(records):
    """Parse and filter valid transactions."""
    return (
        {"id": r["id"], "amount": float(r["amount"]), "type": r["type"]}
        for r in records
        if r.get("amount") and float(r["amount"]) > 0
    )

def transaction_summary(records):
    transactions = load_transactions(records)

    # First pass: count by type
    counts = {}
    for t in transactions:
        counts[t["type"]] = counts.get(t["type"], 0) + 1

    # Second pass: sum by type — BUG: generator already exhausted
    totals = {}
    for t in transactions:
        totals[t["type"]] = totals.get(t["type"], 0) + t["amount"]

    return {"counts": counts, "totals": totals}
```

**What it tests:** Generator exhaustion is a classic Python trap. The second loop gets zero iterations with no error. Agent must realize `transactions` is a generator expression, not a list.

---

### 10. `python-class-attribute-shared` (Level 5)
**Bug:** A class attribute (mutable) is shared across all instances.

```python
class ShoppingCart:
    items = []  # BUG: class attribute, shared across instances
    discount_code = None

    def __init__(self, customer_id):
        self.customer_id = customer_id
        self.discount_code = None  # this one is fine (instance attr)

    def add(self, item, qty=1):
        self.items.append({"item": item, "qty": qty})

    def total(self):
        return sum(i["qty"] for i in self.items)

def process_customers(customer_items):
    results = {}
    for cid, items in customer_items.items():
        cart = ShoppingCart(cid)
        for item, qty in items:
            cart.add(item, qty)
        results[cid] = cart.total()
    return results
```

**What it tests:** `items = []` at class level is shared across all instances. Each customer's cart contains all previous customers' items. The bug is subtle because `discount_code` (immutable) works fine as class attribute, but `items` (mutable list) doesn't. Agent must understand Python's class vs instance attribute mechanics.

---

### 11. `python-deep-pipeline` (Level 5 — Showcase)
**Purpose:** Exercise `debug_evaluate` and `debug_variables` on realistic, complex nested objects. This scenario has the kind of state that makes print-debugging painful — deep object graphs, lists of dicts of lists, intermediate pipeline stages with dozens of fields. The bug is buried in a multi-stage data processing pipeline.

**Setup:** An order fulfillment system that processes customer orders through a pipeline:
1. **Enrichment** — joins order lines with product catalog (prices, weights, categories, warehouse locations)
2. **Shipping** — calculates shipping based on total weight, destination zone, carrier rate tables
3. **Tax** — applies tax rules per category per state (food exempt, clothing taxed above $110, etc.)
4. **Discounts** — applies stacking promotional rules (percentage off, buy-N-get-M, bundle deals)
5. **Finalization** — produces invoice with per-line totals, subtotals, shipping, tax, discounts, grand total

The objects at each stage are realistically large:
- Product catalog: 20+ items with 8+ fields each (sku, name, price, weight, category, warehouse, dimensions, fragile)
- Rate tables: nested dicts keyed by zone → weight_bracket → carrier
- Tax rules: per-state, per-category matrices
- Order: 5-10 line items, each enriched with full product data + computed fields

**Bug:** In the discount stage, a bundle discount checks whether all required SKUs are present in the order. The check uses a set intersection, but it operates on the *enriched* line items which have a `sku` field nested inside a `product` dict. The code does `{line["sku"] for line in order["lines"]}` but after enrichment, the SKU moved to `line["product"]["sku"]`. The top-level `line["sku"]` still exists (it was the original input) but it's the *raw* SKU string, while the bundle rule references *normalized* SKUs (uppercased during enrichment). So the set intersection silently finds no match, and the bundle discount is never applied.

```python
# In enrich_order():
for line in order["lines"]:
    product = catalog[line["sku"]]
    line["product"] = {**product, "sku": product["sku"].upper()}  # normalized
    # line["sku"] still has the original lowercase sku

# In apply_discounts():
def apply_bundle_discount(order, bundle):
    order_skus = {line["sku"] for line in order["lines"]}  # BUG: raw, not normalized
    required = set(bundle["required_skus"])  # these are uppercase
    if required.issubset(order_skus):  # never matches
        # apply discount...
```

**What it tests:**
- Agent must navigate deeply nested objects to find the mismatch
- `debug_evaluate` on expressions like `order["lines"][0].keys()`, `order["lines"][0]["product"]["sku"]` vs `order["lines"][0]["sku"]`
- `debug_variables` will show massive objects — agent must know what to drill into
- The bug is a realistic data pipeline issue: field moved during transformation but downstream code references the old location
- Print-debugging would be excruciating — you'd need to dump enormous objects and visually scan for the SKU field discrepancy

**Files:**
- `pipeline.py` — ~200 lines: the 5-stage pipeline with realistic data structures
- `catalog.py` — product catalog, rate tables, tax rules, bundle promotions (large, realistic data)
- `test_pipeline.py` — visible test: "bundle discount should be applied for qualifying orders"
- `hidden/test_validation.py` — validates bundle discount is correctly applied AND that non-qualifying orders don't get it

**Why this showcases agent-lens:**
An agent without debugging tools would need to read 200+ lines of pipeline code, mentally trace data transformations, and figure out that a SKU normalization in stage 1 isn't accounted for in stage 4. With agent-lens, it can set a breakpoint in `apply_bundle_discount`, inspect `order_skus` vs `required`, see they don't match, then eval `order["lines"][0]["product"]["sku"]` to discover the normalized version exists elsewhere. The eval tool turns a 15-minute investigation into a 30-second one.

---

## Implementation Plan

For each scenario, create:
```
scenarios/<name>/
  scenario.json       # name, language, timeout, budget, test commands
  prompt.md           # natural language bug report (no tool hints)
  src/                # buggy source + visible test
  hidden/             # oracle validation test
```

### 12. `python-encrypted-config` (Level 5 — Contrived)
**Purpose:** A scenario that is nearly impossible without runtime inspection. The bug cannot be found by reading the source code alone — you *must* evaluate expressions at runtime to see what the actual values are.

**Setup:** A configuration system that loads settings from multiple sources (defaults, env vars, config file) and merges them with a priority chain. The config values go through a "normalization" pipeline that includes type coercion, validation, and transformation.

The code is deliberately opaque — values pass through multiple layers of indirection:

```python
import hashlib
import json
import base64

# Registry of transform functions, keyed by config schema version
_TRANSFORMS = {}

def _register(version, key):
    def decorator(fn):
        _TRANSFORMS.setdefault(version, {})[key] = fn
        return fn
    return decorator

@_register("v2", "rate_limit")
def _transform_rate_limit(raw):
    """Normalize rate limit: '100/min' -> {'count': 100, 'window': 60}"""
    parts = raw.split("/")
    windows = {"s": 1, "sec": 1, "min": 60, "m": 60, "hr": 3600, "h": 3600}
    return {"count": int(parts[0]), "window": windows.get(parts[1], 60)}

@_register("v2", "feature_flags")
def _transform_features(raw):
    """Decode feature flags from base64-encoded JSON."""
    if isinstance(raw, str):
        return json.loads(base64.b64decode(raw))
    return raw

@_register("v2", "cache_ttl")
def _transform_cache_ttl(raw):
    """Parse cache TTL with unit suffix."""
    units = {"s": 1, "m": 60, "h": 3600, "d": 86400}
    for suffix, mult in sorted(units.items(), key=lambda x: -len(x[0])):
        if str(raw).endswith(suffix):
            return int(str(raw)[:-len(suffix)]) * mult
    return int(raw)

@_register("v2", "api_key")
def _transform_api_key(raw):
    """Validate and normalize API key format."""
    key = raw.strip()
    # Keys must be 32+ chars, alphanumeric
    if len(key) < 32 or not key[:32].isalnum():
        raise ValueError(f"Invalid API key format: {key[:8]}...")
    return key

def load_config(defaults, overrides, schema_version="v2"):
    """Merge and transform configuration."""
    merged = {**defaults}
    for key, value in overrides.items():
        if key in _TRANSFORMS.get(schema_version, {}):
            merged[key] = _TRANSFORMS[schema_version][key](value)
        else:
            merged[key] = value
    return merged

def compute_cache_key(config):
    """Deterministic cache key from config subset."""
    relevant = {
        "rate_limit": config.get("rate_limit"),
        "cache_ttl": config.get("cache_ttl"),
        "region": config.get("region"),
    }
    raw = json.dumps(relevant, sort_keys=True, default=str)
    return hashlib.sha256(raw.encode()).hexdigest()[:16]

def init_service(defaults, overrides):
    """Initialize service with merged config."""
    config = load_config(defaults, overrides)
    cache_key = compute_cache_key(config)

    # BUG: rate_limit was transformed to a dict {"count": N, "window": M}
    # but the throttle check treats it as the original string
    max_rps = config["rate_limit"]["count"] / config["rate_limit"]["window"]

    return {
        "config": config,
        "cache_key": cache_key,
        "max_rps": max_rps,
        "cache_ttl_seconds": config["cache_ttl"],
        "features": config.get("feature_flags", {}),
    }
```

**The bug:** The defaults dict has `"rate_limit": "50/min"` but one of the override sources provides `"rate_limit": "10/s"`. The `_transform_rate_limit` function parses `"10/s"` correctly to `{"count": 10, "window": 1}`, giving `max_rps = 10.0`. But the *test* expects the service to use the default `"50/min"` rate limit because the override source should have been lower priority. The actual bug is in `load_config`: it unconditionally overwrites defaults with overrides, but the overrides dict contains *all* keys from an env-var scan, including ones set to the string `"10/s"` from a leftover env var. A secondary config file was supposed to take priority over env vars, but the merge order is wrong — env overrides are applied *after* the config file instead of before.

The key insight: **you can't tell which override won by reading the code**. You have to set a breakpoint in `load_config`, inspect the `overrides` dict (which has 15+ keys of transformed and raw values), and trace which source provided `rate_limit`. The value `"10/s"` doesn't appear anywhere in the source files — it comes from the test fixture's env setup.

**What it tests:**
- Agent *must* use `debug_evaluate` to inspect the merged config — the override values are constructed at runtime
- Agent *must* trace through the transform registry to understand what values become after transformation
- Base64-encoded feature flags, hash-derived cache keys, and transform registries make the code resistant to static analysis
- The bug is in *merge order*, not in any single function — requires understanding the data flow across the full init sequence
- Without breakpoints and eval, the agent would need to mentally simulate `load_config` with ~15 key-value pairs, some transformed through registered functions

**Files:**
- `config.py` — ~150 lines: transform registry, load_config, compute_cache_key, init_service
- `test_config.py` — visible test: constructs defaults + env overrides + file overrides, asserts `max_rps == 50/60` (the default rate)
- `hidden/test_validation.py` — validates merge priority: file > env > defaults

---

### Priority order (implement first → last):
1. `python-deep-pipeline` (Level 5 showcase) — the flagship, exercises eval on complex objects
2. `python-encrypted-config` (Level 5 contrived) — impossible without runtime inspection
3. `python-wrong-merge` (Level 2) — first simple new one, tests trace-to-fix
4. `python-shadow-variable` (Level 2) — variable state across loops
5. `python-default-mutable` (Level 3) — Python-specific footgun
6. `python-closure-late-binding` (Level 3) — closure scoping
7. `python-generator-exhaustion` (Level 4) — silent failure
8. `python-dict-iteration-mutation` (Level 4) — mutation during computation
9. `python-float-accumulation` (Level 5) — float precision
10. `python-class-attribute-shared` (Level 5) — class vs instance state

### Timeout / Budget scaling:
- Level 1-2: 120s, $0.50
- Level 3: 180s, $0.75
- Level 4: 240s, $1.00
- Level 5: 300s, $1.50
- Level 5 showcase/contrived: 360s, $2.00

See [scenario-guidelines.md](scenario-guidelines.md) for cross-language design principles, level definitions, and the scenario anatomy checklist.
