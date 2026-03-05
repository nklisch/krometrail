"""Visible failing test — agent can see and run this."""
from transactions import transaction_summary


def test_totals_are_computed():
    records = [
        {"id": "t1", "amount": "100.00", "type": "credit"},
        {"id": "t2", "amount": "50.00", "type": "credit"},
        {"id": "t3", "amount": "25.00", "type": "debit"},
    ]
    result = transaction_summary(records)

    assert result["counts"] == {"credit": 2, "debit": 1}, (
        f"Expected counts credit:2, debit:1 but got {result['counts']}"
    )
    assert result["totals"] == {"credit": 150.00, "debit": 25.00}, (
        f"Expected totals credit:150.00, debit:25.00 but got {result['totals']}"
    )


def test_totals_not_empty_when_records_exist():
    records = [
        {"id": "t1", "amount": "10.00", "type": "credit"},
    ]
    result = transaction_summary(records)
    assert result["totals"], (
        f"totals should not be empty when records exist, got: {result['totals']}"
    )
