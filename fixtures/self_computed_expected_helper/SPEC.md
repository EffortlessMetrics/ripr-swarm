# Fixture: self_computed_expected_helper

Spec: RIPR-SPEC-0094

Related: RIPR-SPEC-0035 (self-computed expected value)

Owner: analysis-fixtures

Issue: #5830

## Given

`tax(subtotal)` changes its rate from 9 to 8 percent. The only test
compares `tax(250)` with `reference_tax(250)`, a `#[cfg(test)]` helper whose
body calls `tax`. The assertion is intentional analyzed fixture input,
governed by the existing `fixtures/**` source-input policy; it is not an
assertion in RIPR's test harness.

The helper returns whatever `tax` returns, so every rate passes the test.
The trap-kit verdict-corpus case `trap-tax-reference-helper` is the same
shape.

## When

```bash
cargo xtask fixtures self_computed_expected_helper
```

## Then

The return-value finding stays `weakly_exposed`. The assertion would pass
the owner-return pin, because `tax(250)` is a complete owner call, but its
expected side reaches `tax` through the helper, so it confirms nothing and
its strength is `weak`.

## Must Not

- Credit an equality whose expected side reaches the changed function
  through a test helper.
