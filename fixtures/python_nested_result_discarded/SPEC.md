# Fixture: python_nested_result_discarded

Spec: RIPR-SPEC-0238

## Given

`tally` defines `inner` and calls it only as a statement of its own, then
returns the constant 2. The diff changes `inner`'s returned value from
`m + 1` to `m + 2`; the only test asserts `tally() == 2`, which passes under
both versions.

## When

```bash
ripr check --root fixtures/python_nested_result_discarded/input --diff fixtures/python_nested_result_discarded/diff.patch
```

## Then

The changed return is `weakly_exposed`: the test reaches `inner` through
`tally`, but nothing `inner` returned reaches what the test asserts.

## Must Not

Credit `tally() == 2` as discriminating the changed nested return, or apply
this to a nested def that is also returned, registered or bound, or whose
return expression calls something.
