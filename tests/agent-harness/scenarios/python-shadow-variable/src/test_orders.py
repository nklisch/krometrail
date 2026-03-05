"""Visible failing test — agent can see and run this."""
from orders import process_orders


def test_grand_total_is_sum_of_all_items():
    orders = [
        {"item": "Widget", "quantity": 2, "price": 10.00},
        {"item": "Gadget", "quantity": 1, "price": 25.00},
        {"item": "Doohickey", "quantity": 3, "price": 5.00},
    ]
    result = process_orders(orders)
    # 2*10 + 1*25 + 3*5 = 20 + 25 + 15 = 60
    assert result["grand_total"] == 60.00, (
        f"Expected grand_total=60.00, got {result['grand_total']}"
    )


def test_single_order():
    orders = [{"item": "Solo", "quantity": 4, "price": 12.50}]
    result = process_orders(orders)
    # 4*12.50 = 50
    assert result["grand_total"] == 50.00, (
        f"Expected grand_total=50.00, got {result['grand_total']}"
    )
