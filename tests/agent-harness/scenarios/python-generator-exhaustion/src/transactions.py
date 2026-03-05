"""Transaction processing utilities."""


def load_transactions(records: list) -> object:
    """Parse and filter valid transactions from raw records.

    Returns a generator of dicts with keys: id, amount, type.
    Only records with a positive amount are included.
    """
    return (
        {"id": r["id"], "amount": float(r["amount"]), "type": r["type"]}
        for r in records
        if r.get("amount") and float(r["amount"]) > 0
    )


def transaction_summary(records: list) -> dict:
    """Summarize transactions by type: count and total amount per type.

    Args:
        records: List of raw transaction dicts with keys: id, amount, type

    Returns:
        Dict with:
            "counts": {type: count} — number of transactions per type
            "totals": {type: total_amount} — sum of amounts per type
    """
    transactions = load_transactions(records)

    # First pass: count by type
    counts: dict = {}
    for t in transactions:
        counts[t["type"]] = counts.get(t["type"], 0) + 1

    # Second pass: sum by type — BUG: generator already exhausted after first loop
    totals: dict = {}
    for t in transactions:
        totals[t["type"]] = totals.get(t["type"], 0.0) + t["amount"]

    return {"counts": counts, "totals": totals}
