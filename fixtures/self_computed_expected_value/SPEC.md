# Fixture: self_computed_expected_value

Spec: RIPR-SPEC-0094

Related: RIPR-SPEC-0035 (self-computed expected value)

Owner: analysis-fixtures

Issue: #5830

## Given

`tax(subtotal)` changes its rate from 9 to 8 percent. `invoice` calls `tax`.
The only test compares `invoice(3, 100)` with `subtotal + tax(subtotal)`, an
expected value it computes through the changed `tax`. The assertion is
intentional analyzed fixture input, governed by the existing `fixtures/**`
source-input policy; it is not an assertion in RIPR's test harness.

Both sides of the equality move with `tax`, so both revisions, and any
other rate, pass the test.

## When

```bash
cargo xtask fixtures self_computed_expected_value
```

The public diff analysis examines the changed return value on `src/lib.rs:6`.

## Then

The return-value finding stays `weakly_exposed`. The assertion calls the
owner and names its parameter, which would otherwise confirm observation,
but its expected side is computed through the owner, so it confirms nothing
and its strength is `weak`.

## Must Not

- Credit an equality whose expected value is computed through the changed
  function as an exact-value discriminator.
