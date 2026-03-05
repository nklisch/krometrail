"""Restaurant bill splitting utility."""


def split_bill(total: float, num_people: int, tip_pct: float = 0.18) -> dict:
    """Split a restaurant bill evenly among num_people, including tip.

    Args:
        total: Pre-tip total in dollars
        num_people: Number of people splitting the bill
        tip_pct: Tip percentage as a decimal (default 18%)

    Returns:
        Dict with:
            "per_person": each person's share, rounded to 2 decimal places
            "shares": list of each person's share (all equal, rounded)
            "total_with_tip": total bill including tip, rounded to 2 decimal places
            "total_shares": sum of all rounded shares
    """
    tip = total * tip_pct
    bill_with_tip = total + tip
    per_person = bill_with_tip / num_people

    # Verify the split adds up exactly
    shares = [per_person] * num_people
    total_shares = sum(shares)

    if total_shares != bill_with_tip:  # BUG: exact float comparison
        # "Correction" that adds floating-point residual to last share,
        # which after rounding makes the last share different from the rest
        shares[-1] += bill_with_tip - total_shares

    return {
        "per_person": round(per_person, 2),
        "shares": [round(s, 2) for s in shares],
        "total_with_tip": round(bill_with_tip, 2),
        "total_shares": round(sum(round(s, 2) for s in shares), 2),
    }
