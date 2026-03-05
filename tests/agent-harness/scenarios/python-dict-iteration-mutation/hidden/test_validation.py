"""Hidden oracle tests — copied into workspace after agent finishes."""
import pytest
from promotions import apply_promotions, run_promotion_campaign


def test_avg_original_price_three_items():
    catalog = {
        "A": {"name": "A", "price": 100.00, "category": "x"},
        "B": {"name": "B", "price": 200.00, "category": "x"},
        "C": {"name": "C", "price": 300.00, "category": "x"},
    }
    result = apply_promotions(catalog, {"A": 50.00, "B": 150.00})
    # Average of originals: (100 + 200 + 300) / 3 = 200
    assert result["avg_original_price"] == pytest.approx(200.00)


def test_avg_original_price_no_promotions():
    catalog = {
        "A": {"name": "A", "price": 100.00, "category": "x"},
        "B": {"name": "B", "price": 200.00, "category": "x"},
    }
    result = apply_promotions(catalog, {})
    # No promotions: average of originals = (100+200)/2 = 150
    assert result["avg_original_price"] == pytest.approx(150.00)


def test_avg_original_price_all_promoted():
    catalog = {
        "A": {"name": "A", "price": 80.00, "category": "x"},
        "B": {"name": "B", "price": 120.00, "category": "x"},
    }
    result = apply_promotions(catalog, {"A": 40.00, "B": 60.00})
    # Average of originals: (80 + 120) / 2 = 100
    assert result["avg_original_price"] == pytest.approx(100.00)


def test_avg_original_price_single_item():
    catalog = {"A": {"name": "A", "price": 50.00, "category": "x"}}
    result = apply_promotions(catalog, {"A": 25.00})
    assert result["avg_original_price"] == pytest.approx(50.00)


def test_total_savings():
    catalog = {
        "A": {"name": "A", "price": 100.00, "category": "x"},
        "B": {"name": "B", "price": 200.00, "category": "x"},
    }
    result = apply_promotions(catalog, {"A": 75.00, "B": 180.00})
    assert result["total_savings"] == pytest.approx(45.00)


def test_updated_count():
    catalog = {
        "A": {"name": "A", "price": 100.00, "category": "x"},
        "B": {"name": "B", "price": 200.00, "category": "x"},
        "C": {"name": "C", "price": 300.00, "category": "x"},
    }
    result = apply_promotions(catalog, {"A": 50.00, "NONEXISTENT": 10.00})
    assert result["updated"] == 1


def test_promotion_for_nonexistent_sku_ignored():
    catalog = {"A": {"name": "A", "price": 100.00, "category": "x"}}
    result = apply_promotions(catalog, {"MISSING": 50.00})
    assert result["updated"] == 0
    assert result["avg_original_price"] == pytest.approx(100.00)


def test_run_promotion_campaign():
    catalog = {
        "A": {"name": "Widget", "price": 100.00, "category": "x"},
        "B": {"name": "Gadget", "price": 200.00, "category": "y"},
    }
    report = run_promotion_campaign(catalog, {"A": 70.00})
    assert report["summary"]["updated"] == 1
    assert report["summary"]["avg_original_price"] == pytest.approx(150.00)
    assert report["catalog_size"] == 2
    assert report["promoted_items"][0]["original_price"] == 100.00
    assert report["promoted_items"][0]["promo_price"] == 70.00


def test_regression_prices_not_mutated_before_averaging():
    """Regression: avg_original_price must use pre-mutation prices."""
    catalog = {
        "EXPENSIVE": {"name": "E", "price": 1000.00, "category": "x"},
        "CHEAP": {"name": "C", "price": 10.00, "category": "x"},
    }
    # Drop expensive item to 10
    result = apply_promotions(catalog, {"EXPENSIVE": 10.00})
    # Average of originals: (1000 + 10) / 2 = 505
    # If bug present: (10 + 10) / 2 = 10
    assert result["avg_original_price"] == pytest.approx(505.00), (
        f"avg_original_price should be 505.00 (from originals), got {result['avg_original_price']}"
    )
