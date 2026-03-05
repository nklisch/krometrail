"""Sales tracking and reporting module."""


def register_sale(item: str, price: float, ledger: list = []) -> list:
    """Record a sale in the ledger and return the updated ledger.

    Args:
        item: Name of the item sold.
        price: Sale price.
        ledger: The ledger to append to. Defaults to a new empty ledger.

    Returns:
        The ledger list with the new sale appended.
    """
    # BUG: The default `ledger=[]` is a mutable default argument.
    # Python creates the default list ONCE at function definition time,
    # so all calls that use the default share the same list object.
    # Each "new" ledger actually accumulates sales from every previous call.
    ledger.append({"item": item, "price": price})
    return ledger


def daily_report(sales_by_day: list[list[tuple[str, float]]]) -> list[dict]:
    """Generate a sales report for each day.

    Args:
        sales_by_day: A list of days, where each day is a list of
                      (item_name, price) tuples.

    Returns:
        A list of daily report dicts, each containing:
            - "day": 1-indexed day number
            - "count": number of sales that day
            - "total": total revenue for that day
            - "items": list of item names sold that day
    """
    reports = []
    for day_index, day_sales in enumerate(sales_by_day):
        # Start a fresh ledger for each day by calling without the ledger arg
        ledger = register_sale(day_sales[0][0], day_sales[0][1])
        for item, price in day_sales[1:]:
            register_sale(item, price, ledger)

        reports.append({
            "day": day_index + 1,
            "count": len(ledger),
            "total": sum(sale["price"] for sale in ledger),
            "items": [sale["item"] for sale in ledger],
        })

    return reports


def weekly_summary(sales_by_day: list[list[tuple[str, float]]]) -> dict:
    """Generate a weekly summary from daily reports.

    Returns:
        Dict with "days", "total_sales", "total_revenue", "best_day".
    """
    reports = daily_report(sales_by_day)
    if not reports:
        return {"days": 0, "total_sales": 0, "total_revenue": 0.0, "best_day": None}

    best = max(reports, key=lambda r: r["total"])
    return {
        "days": len(reports),
        "total_sales": sum(r["count"] for r in reports),
        "total_revenue": sum(r["total"] for r in reports),
        "best_day": best["day"],
    }
