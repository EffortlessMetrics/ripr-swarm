# Fixture: same_line_call_inputs_rival

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #13373

## Given

The diff changes only `a::gate` from `>` to `>=`. A test asserts the owner's
far input and a same-named sibling's boundary input on one source line.

## When

```bash
cargo xtask fixtures same_line_call_inputs_rival
```

## Then

The changed predicate must remain below `exposed`: the sibling's boundary input
cannot close the changed owner's boundary. Existing identity refusal remains
conservative when a line mixes owners.

## Must Not

- Borrow `b::gate(10)` as an input to `a::gate`.
- Bless an `exposed` golden for this negative control.
