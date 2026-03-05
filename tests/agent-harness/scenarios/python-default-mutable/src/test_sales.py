"""Visible failing test — agent can see and run this."""
from sales import daily_report


def test_each_day_independent():
    sales = [
        [("Coffee", 4.50), ("Muffin", 3.00)],       # Day 1: 2 items, $7.50
        [("Sandwich", 8.00), ("Juice", 3.50)],       # Day 2: 2 items, $11.50
        [("Salad", 9.00)],                            # Day 3: 1 item,  $9.00
    ]
    reports = daily_report(sales)

    assert reports[0]["count"] == 2, (
        f"Day 1 should have 2 sales, got {reports[0]['count']}"
    )
    assert reports[1]["count"] == 2, (
        f"Day 2 should have 2 sales, got {reports[1]['count']}"
    )
    assert reports[2]["count"] == 1, (
        f"Day 3 should have 1 sale, got {reports[2]['count']}"
    )


def test_day_totals_independent():
    sales = [
        [("A", 10.00), ("B", 20.00)],   # Day 1: $30
        [("C", 5.00)],                    # Day 2: $5
    ]
    reports = daily_report(sales)

    assert reports[0]["total"] == 30.00, (
        f"Day 1 total should be 30.00, got {reports[0]['total']}"
    )
    assert reports[1]["total"] == 5.00, (
        f"Day 2 total should be 5.00, got {reports[1]['total']}"
    )
