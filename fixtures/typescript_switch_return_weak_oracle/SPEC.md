# Fixture: typescript_switch_return_weak_oracle

Spec: RIPR-SPEC-0243

## Given

Two single-function TypeScript modules each contain one `switch` on their
sole string parameter, with unique string case labels and one numeric
`return` per arm. In both modules the `silver` arm changes `return 5;` to
`return 7;` as a single-line replacement (issue #12951).

- `src/price.ts`: the only related test directly calls `price('silver')`
  and holds the result in a `const` local checked with
  `expect(result).toBeLessThan(100)`, a weak relational oracle that both
  the old (`5`) and new (`7`) values pass.
- `src/rate.ts` (wrong-arm negative control): the only related test
  directly calls `rate('gold')`, never the changed `silver` arm, with the
  same weak oracle shape.

## When

```bash
ripr check \
  --root fixtures/typescript_switch_return_weak_oracle/input \
  --diff fixtures/typescript_switch_return_weak_oracle/diff.patch \
  --mode fast --json
```

## Then

- the selected `silver` arm in `price` reads `weakly_exposed` with a
  missing discriminator `return value == 7`: the test input enters the
  changed arm, but `toBeLessThan(100)` cannot tell `5` from `7`;
- the wrong-arm control in `rate` stays non-promoted
  (`reachable_unrevealed`, infection `no (branch_not_entered)`): no test
  input enters its changed arm, so the bounded switch-return witness
  must not credit it.

## Must Not

- Promote a switch arm no selected test input enters.
- Turn the weak relational oracle into `exposed`, or claim a runtime
  mutation outcome from static evidence.
- Promote any other literal-return shape through this witness.
