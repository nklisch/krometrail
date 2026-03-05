"""Hidden oracle tests — copied into workspace after agent finishes."""
import pytest
from orders import process_orders, summarize_orders


def test_grand_total_multiple_items():
    orders = [
        {"item": "A", "quantity": 2, "price": 10.00},
        {"item": "B", "quantity": 1, "price": 25.00},
        {"item": "C", "quantity": 3, "price": 5.00},
    ]
    result = process_orders(orders)
    assert result["grand_total"] == pytest.approx(60.00), (
        f"Expected 60.00, got {result['grand_total']}"
    )


def test_grand_total_single_item():
    orders = [{"item": "X", "quantity": 4, "price": 12.50}]
    result = process_orders(orders)
    assert result["grand_total"] == pytest.approx(50.00)


def test_grand_total_two_items():
    orders = [
        {"item": "A", "quantity": 1, "price": 100.00},
        {"item": "B", "quantity": 1, "price": 200.00},
    ]
    result = process_orders(orders)
    assert result["grand_total"] == pytest.approx(300.00)


def test_line_totals_correct():
    orders = [
        {"item": "A", "quantity": 2, "price": 10.00},
        {"item": "B", "quantity": 3, "price": 5.00},
    ]
    result = process_orders(orders)
    assert result["line_totals"] == [20.00, 15.00]


def test_item_count():
    orders = [
        {"item": "A", "quantity": 1, "price": 1.00},
        {"item": "B", "quantity": 1, "price": 2.00},
        {"item": "C", "quantity": 1, "price": 3.00},
    ]
    result = process_orders(orders)
    assert result["item_count"] == 3


def test_empty_orders():
    result = process_orders([])
    assert result["grand_total"] == 0
    assert result["item_count"] == 0


def test_grand_total_not_doubled():
    """Regression: total should not include leftover value from validation pass."""
    orders = [
        {"item": "Expensive", "quantity": 1, "price": 999.00},
        {"item": "Cheap", "quantity": 1, "price": 1.00},
    ]
    result = process_orders(orders)
    # If total isn't reset, the last validation value (1*1=1) leaks into the sum
    assert result["grand_total"] == pytest.approx(1000.00), (
        f"Grand total should be exactly 1000.00, got {result['grand_total']}"
    )


def test_summarize_output():
    orders = [{"item": "Widget", "quantity": 2, "price": 10.00}]
    summary = summarize_orders(orders)
    assert "$20.00" in summary
