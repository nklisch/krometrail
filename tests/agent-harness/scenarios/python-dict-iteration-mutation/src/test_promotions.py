"""Visible failing test — agent can see and run this."""
from promotions import apply_promotions


def test_avg_original_price_unaffected_by_promotions():
    catalog = {
        "SKU-001": {"name": "Widget A", "price": 100.00, "category": "electronics"},
        "SKU-002": {"name": "Widget B", "price": 200.00, "category": "electronics"},
        "SKU-003": {"name": "Gadget C", "price": 300.00, "category": "accessories"},
    }
    promotions = {
        "SKU-001": 50.00,   # 100 -> 50
        "SKU-002": 150.00,  # 200 -> 150
    }

    result = apply_promotions(catalog, promotions)

    # Average of ORIGINAL prices: (100 + 200 + 300) / 3 = 200.00
    assert result["avg_original_price"] == 200.00, (
        f"Expected avg_original_price=200.00 (from original prices 100+200+300), "
        f"got {result['avg_original_price']}"
    )


def test_total_savings_correct():
    catalog = {
        "SKU-001": {"name": "A", "price": 100.00, "category": "x"},
        "SKU-002": {"name": "B", "price": 200.00, "category": "x"},
    }
    promotions = {
        "SKU-001": 75.00,   # saves 25
        "SKU-002": 180.00,  # saves 20
    }

    result = apply_promotions(catalog, promotions)
    assert result["total_savings"] == 45.00, (
        f"Expected total_savings=45.00, got {result['total_savings']}"
    )
