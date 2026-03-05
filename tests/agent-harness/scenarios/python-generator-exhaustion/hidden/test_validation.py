"""Hidden oracle tests — copied into workspace after agent finishes."""
import pytest
from transactions import transaction_summary, load_transactions


def test_counts_and_totals_both_populated():
    records = [
        {"id": "t1", "amount": "100.00", "type": "credit"},
        {"id": "t2", "amount": "50.00", "type": "debit"},
    ]
    result = transaction_summary(records)
    assert result["counts"] == {"credit": 1, "debit": 1}
    assert result["totals"] == {"credit": 100.00, "debit": 50.00}


def test_totals_multiple_per_type():
    records = [
        {"id": "t1", "amount": "100.00", "type": "credit"},
        {"id": "t2", "amount": "50.00", "type": "credit"},
        {"id": "t3", "amount": "25.00", "type": "debit"},
    ]
    result = transaction_summary(records)
    assert result["totals"]["credit"] == pytest.approx(150.00)
    assert result["totals"]["debit"] == pytest.approx(25.00)


def test_negative_amounts_excluded():
    records = [
        {"id": "t1", "amount": "100.00", "type": "credit"},
        {"id": "t2", "amount": "-10.00", "type": "credit"},  # excluded
    ]
    result = transaction_summary(records)
    assert result["counts"] == {"credit": 1}
    assert result["totals"] == {"credit": 100.00}


def test_zero_amount_excluded():
    records = [
        {"id": "t1", "amount": "0", "type": "credit"},  # excluded (falsy)
        {"id": "t2", "amount": "5.00", "type": "debit"},
    ]
    result = transaction_summary(records)
    assert "credit" not in result["counts"]
    assert result["totals"]["debit"] == pytest.approx(5.00)


def test_missing_amount_excluded():
    records = [
        {"id": "t1", "type": "credit"},  # no amount key -> excluded
        {"id": "t2", "amount": "20.00", "type": "debit"},
    ]
    result = transaction_summary(records)
    assert "credit" not in result["counts"]
    assert result["totals"]["debit"] == pytest.approx(20.00)


def test_empty_records():
    result = transaction_summary([])
    assert result["counts"] == {}
    assert result["totals"] == {}


def test_single_transaction():
    records = [{"id": "t1", "amount": "42.00", "type": "credit"}]
    result = transaction_summary(records)
    assert result["counts"] == {"credit": 1}
    assert result["totals"] == {"credit": pytest.approx(42.00)}


def test_counts_match_totals_keys():
    records = [
        {"id": "t1", "amount": "10.00", "type": "credit"},
        {"id": "t2", "amount": "20.00", "type": "debit"},
        {"id": "t3", "amount": "30.00", "type": "credit"},
    ]
    result = transaction_summary(records)
    assert set(result["counts"].keys()) == set(result["totals"].keys())


def test_load_transactions_can_be_iterated_twice():
    """load_transactions must return something iterable more than once (e.g. a list)."""
    records = [
        {"id": "t1", "amount": "10.00", "type": "credit"},
    ]
    txns = load_transactions(records)
    first = list(txns)
    second = list(txns)
    assert first == second, (
        "load_transactions result was exhausted after first iteration — must return a list, not a generator"
    )
