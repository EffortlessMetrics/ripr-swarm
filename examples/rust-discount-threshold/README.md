# Rust: discount threshold

`project/src/lib.rs` gives a discount to orders at or above
`DISCOUNT_THRESHOLD`. [`change.diff`](change.diff) is the change that made the
boundary inclusive (`>` became `>=`). The tests in `project/tests/pricing.rs`
check 9,999 and 10,001, so they pass with either comparison.

## See the finding

From this directory:

```bash
ripr check --root project --diff change.diff
```

Selected lines of the output:

```text
Start here:
  State: a test gap to inspect or repair
  File: ./src/lib.rs:5
  Static exposure: weak (severity: warning, confidence 0.92)
  Changed behavior: subtotal >= DISCOUNT_THRESHOLD
  Missing check: subtotal == DISCOUNT_THRESHOLD
  Next step: Add boundary tests for below, equal, and above the changed threshold with exact assertions
```

## Fix it

[`strengthened/pricing.rs`](strengthened/pricing.rs) is the same test file with
one more test, `exactly_at_threshold_is_discounted`, which asserts
`discounted_total(10_000) == 9_500`. Put it in place and check again:

```bash
cp strengthened/pricing.rs project/tests/pricing.rs
ripr check --root project --diff change.diff
```

The summary now reads `1 appear noticed by tests` and the finding is `exposed`.
To run the tests themselves: `cargo test --manifest-path project/Cargo.toml`.

Restore the starting point with `git checkout -- project/tests/pricing.rs`.
