# Fixture: owner_name_collision_helper_keeps_credit

Spec: RIPR-SPEC-0094

Related: RIPR-SPEC-0035 (self-computed expected value)

Owner: analysis-fixtures

Issue: #5830

## Given

`Stack::len` changes to return one more than the item count. The only test
compares `stack.len()` with `count_items(&[1, 2, 3, 4])`, a production
helper whose body calls `values.len()` on a slice. The assertion is
intentional analyzed fixture input, governed by the existing `fixtures/**`
source-input policy; it is not an assertion in RIPR's test harness.

The helper shares the owner's name only through the slice's own `len`
method; it never calls `Stack::len`, so its result is independent of the
change and the equality discriminates it.

## When

```bash
cargo xtask fixtures owner_name_collision_helper_keeps_credit
```

## Then

The return-value finding stays `exposed`. The self-computed-expected rule
withholds credit only when the expected side reaches the owner through a
call that can name it (`Stack::len`, `Self::len` or `self.len()` inside
`impl Stack`); `values.len()` is an unresolved method call and does not.

## Must Not

- Withhold credit because a helper calls a foreign method with the owner's
  name.
