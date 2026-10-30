# Fixture: invisible_change_relational_sibling

Spec: RIPR-SPEC-0197

## Given

`backoff_ms` multiplies by `2u64.pow(attempt)` and the diff changes the base to
`3u64.pow(attempt)`. The related tests assert the exact first attempt and a
relational sibling: `backoff_ms(3, 100) > backoff_ms(1, 100)`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: the relational sibling is true under
both versions (`800 > 200` before, `2700 > 300` after), so no assertion's
outcome tells the versions apart (#10916). The infection summary names the
unchanged comparison truth rather than a missing input difference, because the
asserted inputs do evaluate differently.

## Must Not

- Promote the change to `exposed` from a relational assertion whose comparison
  folds to the same truth value under the changed and the baseline expression.
