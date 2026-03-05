"""Visible failing test — agent can see and run this."""
from bill import split_bill


def test_total_shares_matches_total_with_tip():
    # $47.00 bill, 3 people, 18% tip
    # bill_with_tip = $55.46, per_person = $18.4866...
    # Rounded shares: [18.49, 18.49, 18.49] sum to $55.47 != $55.46
    result = split_bill(47.00, 3)
    assert result["total_shares"] == result["total_with_tip"], (
        f"total_shares {result['total_shares']} != total_with_tip {result['total_with_tip']} — "
        f"shares {result['shares']} don't sum to the expected total"
    )


def test_total_shares_matches_total_with_tip_six_people():
    # $53.00 bill, 6 people — also triggers the discrepancy
    result = split_bill(53.00, 6)
    assert result["total_shares"] == result["total_with_tip"], (
        f"total_shares {result['total_shares']} != total_with_tip {result['total_with_tip']}"
    )


def test_total_shares_correct_no_tip():
    # Simple case: $30 / 3 people, no tip — should always work
    result = split_bill(30.00, 3, tip_pct=0.0)
    assert result["total_shares"] == 30.00
    assert result["total_with_tip"] == 30.00
