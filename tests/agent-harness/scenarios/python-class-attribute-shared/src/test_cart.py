"""Visible failing test — agent can see and run this."""
from cart import ShoppingCart, process_customers


def test_separate_carts_do_not_share_items():
    cart_a = ShoppingCart("alice")
    cart_a.add("apple", 2)

    cart_b = ShoppingCart("bob")
    cart_b.add("banana", 1)

    assert cart_a.total_items() == 2, (
        f"Alice's cart should have 2 items, got {cart_a.total_items()}"
    )
    assert cart_b.total_items() == 1, (
        f"Bob's cart should have 1 item, got {cart_b.total_items()}"
    )


def test_process_customers_returns_correct_totals():
    customer_items = {
        "alice": [("apple", 3)],
        "bob": [("banana", 2), ("cherry", 1)],
    }
    result = process_customers(customer_items)

    assert result["alice"] == 3, (
        f"Alice should have 3 items, got {result['alice']}"
    )
    assert result["bob"] == 3, (
        f"Bob should have 3 items, got {result['bob']}"
    )
