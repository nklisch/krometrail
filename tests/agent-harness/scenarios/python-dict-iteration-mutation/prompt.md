The `apply_promotions` function in `promotions.py` is returning the wrong `avg_original_price`. It should report the average of the original catalog prices before promotions were applied, but the value is lower than expected.

The test in `test_promotions.py` demonstrates the failure. Debug this issue and fix the bug so that `test_promotions.py` passes.
