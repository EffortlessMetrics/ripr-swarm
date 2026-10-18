# Fixture: field_match_helper_mutable

Spec: RIPR-SPEC-0229

## Given

A test obtains a local record through two statement-free helpers and writes its selected field before the exact asserted owner call.

## When

`cargo xtask fixtures field_match_helper_mutable`

## Then

The changed Some arm is exposed only through its own admitted equality on the same call.

## Must Not

Borrow a field value from another invocation, owner, field or invalidated binding.
