# Python: bulk discount (preview)

Python analysis is a preview: its evidence is advisory and is not on the same
scale as Rust evidence. No `ripr.toml` is needed; `ripr` enables the preview
because `project/` has a `pyproject.toml`.

`project/pricing/core.py` gives a bulk discount to orders of 20 units or more.
[`change.diff`](change.diff) is the change that made the boundary inclusive
(`>` became `>=`). The tests in `project/tests/test_pricing.py` check 19 and
21, so they pass with either comparison.

## See the finding

From this directory:

```bash
ripr check --root project --diff change.diff
```

Selected lines of the output:

```text
Start here:
  State: preview language, advisory only
  File: pricing/core.py:3
  Language: python
  Static exposure: weak (severity: warning, confidence 0.40)
  Changed behavior: if units >= 20:
  Missing check: A strong Python oracle reaches `bulk_discount_rate`, but static evidence does not
    place a related test input at the changed predicate boundary `units == 20`. No strong related
    test call places units equal to 20; observed units values: 19, 21
```

## Fix it

[`strengthened/test_pricing.py`](strengthened/test_pricing.py) is the same test
file with one more test, `test_exactly_at_boundary_gets_discount`, which
asserts `bulk_discount_rate(20) == 0.15`. Put it in place and check again:

```bash
cp strengthened/test_pricing.py project/tests/test_pricing.py
ripr check --root project --diff change.diff
```

The summary now reads `1 appear noticed by tests` and the finding is `exposed`.
To run the tests themselves: `cd project && python -m pytest`.

Restore the starting point with
`git checkout -- project/tests/test_pricing.py`.
