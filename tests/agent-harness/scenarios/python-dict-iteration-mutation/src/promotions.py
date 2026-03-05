"""Catalog promotion engine.

Applies promotional pricing to a product catalog and computes statistics
about the original vs promotional prices.
"""


def apply_promotions(catalog: dict, promotions: dict) -> dict:
    """Apply promotional prices to catalog items and return a summary.

    For each item in `promotions`, if the SKU exists in the catalog:
        - Sets the item's price to the promotional price
        - Records the savings (original - promotional)

    After applying promotions, computes the average original price
    across ALL items in the catalog (promoted and non-promoted).

    Args:
        catalog: Dict mapping SKU -> {"name": str, "price": float, "category": str}
        promotions: Dict mapping SKU -> promotional_price (float)

    Returns:
        Dict with:
            "updated": number of items that received a promotion
            "avg_original_price": average price BEFORE promotions were applied
            "total_savings": sum of savings across all promoted items
    """
    updated = 0
    total_savings = 0.0

    for sku, promo_price in promotions.items():
        if sku in catalog:
            old_price = catalog[sku]["price"]
            catalog[sku]["price"] = promo_price
            catalog[sku]["savings"] = old_price - promo_price
            total_savings += old_price - promo_price
            updated += 1

    # BUG: At this point, catalog[sku]["price"] has already been overwritten
    # with the promotional price for promoted items. This list comprehension
    # reads the mutated prices, not the original ones.
    original_prices = [item["price"] for item in catalog.values()]
    avg_original = sum(original_prices) / len(original_prices) if original_prices else 0.0

    return {
        "updated": updated,
        "avg_original_price": round(avg_original, 2),
        "total_savings": round(total_savings, 2),
    }


def run_promotion_campaign(catalog: dict, promotions: dict) -> dict:
    """Run a promotion campaign and return a full report.

    Returns:
        Dict with "summary" (from apply_promotions), "promoted_items" (list of
        promoted SKUs with before/after prices), and "catalog_size".
    """
    # Snapshot promoted items before applying
    promoted_items = []
    for sku, promo_price in promotions.items():
        if sku in catalog:
            promoted_items.append({
                "sku": sku,
                "name": catalog[sku]["name"],
                "original_price": catalog[sku]["price"],
                "promo_price": promo_price,
            })

    summary = apply_promotions(catalog, promotions)

    return {
        "summary": summary,
        "promoted_items": promoted_items,
        "catalog_size": len(catalog),
    }
