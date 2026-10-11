# Fixture: owner_parameter_token_coincidence

Spec: RIPR-SPEC-0094

Owner: analysis-fixtures

Issue: #5830

## Given

`tax(subtotal)` changes its rate from 9 to 8 percent. `tax_is_owed` calls
`tax` but only checks that the result is positive. `subtotal_multiplies`
pins `subtotal(3, 100)` exactly; it never calls `tax`. Its text shares two
words with the changed line: `subtotal`, which there names a different
function, and `100`, which there is an input. These assertions are
intentional analyzed fixture input, governed by the existing `fixtures/**`
source-input policy; they are not assertions in RIPR's test harness.

Both revisions pass both tests, so no test discriminates the change.

## When

```bash
cargo xtask fixtures owner_parameter_token_coincidence
```

The public diff analysis examines the changed return value on `src/lib.rs:6`.

## Then

The return-value finding stays `weakly_exposed`. The owner's parameter name
and the numeric literal confirm observation only in an assertion that calls
`tax`, so `subtotal_multiplies` cannot supply the confirmation.

## Must Not

- Confirm observation from a word that names the owner's parameter in an
  assertion that never calls the owner.
- Confirm observation from a numeric test input that matches a literal in
  the changed expression.
