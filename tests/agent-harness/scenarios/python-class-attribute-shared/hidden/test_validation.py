"""Hidden oracle tests — copied into workspace after agent finishes."""
import pytest
from cart import ShoppingCart, process_customers


def test_two_carts_independent():
    cart_a = ShoppingCart("a")
    cart_a.add("apple", 2)

    cart_b = ShoppingCart("b")
    cart_b.add("banana", 1)

    assert cart_a.total_items() == 2
    assert cart_b.total_items() == 1


def test_three_carts_independent():
    c1 = ShoppingCart("c1")
    c1.add("x", 1)

    c2 = ShoppingCart("c2")
    c2.add("y", 2)

    c3 = ShoppingCart("c3")
    c3.add("z", 3)

    assert c1.total_items() == 1, f"c1 should have 1 item, got {c1.total_items()}"
    assert c2.total_items() == 2, f"c2 should have 2 items, got {c2.total_items()}"
    assert c3.total_items() == 3, f"c3 should have 3 items, got {c3.total_items()}"


def test_process_customers_single():
    result = process_customers({"alice": [("apple", 5)]})
    assert result["alice"] == 5


def test_process_customers_multiple():
    customer_items = {
        "alice": [("apple", 3)],
        "bob": [("banana", 2), ("cherry", 1)],
        "carol": [("date", 4), ("elderberry", 2)],
    }
    result = process_customers(customer_items)
    assert result["alice"] == 3
    assert result["bob"] == 3
    assert result["carol"] == 6


def test_cart_items_not_leaked_between_process_calls():
    result1 = process_customers({"a": [("x", 1)]})
    result2 = process_customers({"b": [("y", 2)]})
    assert result1["a"] == 1
    assert result2["b"] == 2


def test_empty_cart():
    result = process_customers({"alice": []})
    assert result["alice"] == 0


def test_item_names_isolated():
    cart_a = ShoppingCart("a")
    cart_a.add("apple")

    cart_b = ShoppingCart("b")
    cart_b.add("banana")

    assert cart_a.item_names() == ["apple"], f"a's items: {cart_a.item_names()}"
    assert cart_b.item_names() == ["banana"], f"b's items: {cart_b.item_names()}"


def test_discount_code_per_instance():
    cart_a = ShoppingCart("a")
    cart_a.discount_code = "SAVE10"

    cart_b = ShoppingCart("b")
    assert cart_b.discount_code is None, (
        f"cart_b.discount_code should be None, got {cart_b.discount_code}"
    )


def test_regression_class_attribute_not_shared():
    """Regression: ShoppingCart.items must be per-instance, not shared at class level."""
    c1 = ShoppingCart("c1")
    c1.add("item", 1)
    c2 = ShoppingCart("c2")
    # c2 should start empty
    assert c2.total_items() == 0, (
        f"New cart should be empty but has {c2.total_items()} items — class attribute shared!"
    )
