"""Shopping cart system."""


class ShoppingCart:
    items = []  # BUG: class attribute — shared across all instances
    discount_code = None  # This one is fine (reassigned in __init__, so per-instance)

    def __init__(self, customer_id: str):
        self.customer_id = customer_id
        self.discount_code = None  # creates instance attribute, shadows class attr

    def add(self, item: str, qty: int = 1) -> None:
        """Add an item to the cart."""
        self.items.append({"item": item, "qty": qty})

    def total_items(self) -> int:
        """Return the total quantity of items in the cart."""
        return sum(i["qty"] for i in self.items)

    def item_names(self) -> list:
        """Return a list of item names in the cart."""
        return [i["item"] for i in self.items]


def process_customers(customer_items: dict) -> dict:
    """Create a cart for each customer and return their item totals.

    Args:
        customer_items: {customer_id: [(item_name, qty), ...]}

    Returns:
        {customer_id: total_item_count}
    """
    results = {}
    for cid, items in customer_items.items():
        cart = ShoppingCart(cid)
        for item, qty in items:
            cart.add(item, qty)
        results[cid] = cart.total_items()
    return results
