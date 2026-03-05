The `process_customers` function in `cart.py` is returning wrong totals for each customer. Instead of each customer's cart containing only their own items, the totals grow with each successive customer — as if all customers are sharing a single cart.

The test in `test_cart.py` demonstrates the failure. Debug this issue and fix the bug so that `test_cart.py` passes.
