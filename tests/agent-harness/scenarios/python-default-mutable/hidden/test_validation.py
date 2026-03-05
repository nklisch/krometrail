"""Hidden oracle tests — copied into workspace after agent finishes."""
import pytest
from sales import daily_report, register_sale, weekly_summary


def test_day_counts_independent():
    sales = [
        [("Coffee", 4.50), ("Muffin", 3.00)],
        [("Sandwich", 8.00), ("Juice", 3.50)],
        [("Salad", 9.00)],
    ]
    reports = daily_report(sales)
    assert reports[0]["count"] == 2
    assert reports[1]["count"] == 2
    assert reports[2]["count"] == 1


def test_day_totals_independent():
    sales = [
        [("A", 10.00), ("B", 20.00)],
        [("C", 5.00)],
    ]
    reports = daily_report(sales)
    assert reports[0]["total"] == pytest.approx(30.00)
    assert reports[1]["total"] == pytest.approx(5.00)


def test_day_items_independent():
    sales = [
        [("X", 1.00)],
        [("Y", 2.00)],
    ]
    reports = daily_report(sales)
    assert reports[0]["items"] == ["X"], f"Day 1 items: {reports[0]['items']}"
    assert reports[1]["items"] == ["Y"], f"Day 2 items: {reports[1]['items']}"


def test_register_sale_no_cross_contamination():
    """Calling register_sale without a ledger twice should give independent ledgers."""
    ledger1 = register_sale("item1", 10.00)
    ledger2 = register_sale("item2", 20.00)
    # If default mutable is fixed, these should be different lists
    assert ledger1 is not ledger2 or len(ledger1) == 1, (
        f"Ledgers should be independent. ledger1={ledger1}, ledger2={ledger2}"
    )


def test_weekly_summary_correct():
    sales = [
        [("A", 10.00)],
        [("B", 20.00), ("C", 30.00)],
    ]
    summary = weekly_summary(sales)
    assert summary["days"] == 2
    assert summary["total_sales"] == 3
    assert summary["total_revenue"] == pytest.approx(60.00)
    assert summary["best_day"] == 2


def test_five_days_no_accumulation():
    """Five days of single sales — count and total should not grow."""
    sales = [[(f"item{i}", 10.00)] for i in range(5)]
    reports = daily_report(sales)
    for i, r in enumerate(reports):
        assert r["count"] == 1, f"Day {i+1} count should be 1, got {r['count']}"
        assert r["total"] == pytest.approx(10.00), (
            f"Day {i+1} total should be 10.00, got {r['total']}"
        )


def test_called_twice_independently():
    """Calling daily_report twice should produce the same results."""
    sales = [[("X", 5.00)], [("Y", 10.00)]]
    reports1 = daily_report(sales)
    reports2 = daily_report(sales)
    assert reports1[0]["count"] == reports2[0]["count"], (
        f"Second call gave different results: {reports1} vs {reports2}"
    )
