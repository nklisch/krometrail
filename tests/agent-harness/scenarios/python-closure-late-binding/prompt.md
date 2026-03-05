The `make_validators` function in `validators.py` is supposed to create range validators with different bounds, but all validators seem to use the same range regardless of how they were configured.

The test in `test_validators.py` demonstrates the failure. Debug this issue and fix the bug so that `test_validators.py` passes.
