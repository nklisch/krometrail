The `transaction_summary` function in `transactions.py` is returning empty totals even when there are valid transactions. The `counts` field correctly shows how many transactions exist per type, but the `totals` field is always an empty dict `{}`.

The test in `test_transactions.py` demonstrates the failure. Debug this issue and fix the bug so that `test_transactions.py` passes.
