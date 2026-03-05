"""Order processing module."""


def validate_line_item(order: dict) -> float:
    """Check that a line item has valid quantity and price. Returns the line total."""
    qty = order["quantity"]
    price = order["price"]
    if qty < 0:
        raise ValueError(f"Negative quantity: {qty}")
    if price < 0:
        raise ValueError(f"Negative price: {price}")
    return qty * price


def process_orders(orders: list[dict]) -> dict:
    """Process a list of order line items.

    First validates all line items to ensure no negative quantities or prices,
    then accumulates the grand total across all items.

    Args:
        orders: List of dicts with "item", "quantity", and "price" keys.

    Returns:
        Dict with "item_count", "grand_total", and "line_totals".
    """
    line_totals = []

    # First pass: validate every line item
    total = 0
    for order in orders:
        total = validate_line_item(order)
        line_totals.append(total)

    # Second pass: accumulate grand total
    # BUG: `total` still holds the last validation result instead of 0
    for order in orders:
        total += order["quantity"] * order["price"]

    return {
        "item_count": len(orders),
        "grand_total": total,
        "line_totals": line_totals,
    }


def summarize_orders(orders: list[dict]) -> str:
    """Return a human-readable summary of processed orders."""
    result = process_orders(orders)
    lines = [f"Items: {result['item_count']}", f"Total: ${result['grand_total']:.2f}"]
    for i, lt in enumerate(result["line_totals"]):
        lines.append(f"  Line {i + 1}: ${lt:.2f}")
    return "\n".join(lines)
