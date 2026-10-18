# Fixture: field_match_helper_taken

Spec: RIPR-SPEC-0229

## Given

A field method takes the selected value before the asserted owner call; the earlier selecting write cannot supply its selector.

## When

`cargo xtask fixtures field_match_helper_taken`

## Then

The changed Some arm is not exposed; the implicit mutable borrow invalidates the earlier field value.

## Must Not

Borrow a field value from another invocation, owner, field or invalidated binding.
