# Fixture: rust_display_placeholder_edit

Spec: RIPR-SPEC-0297

## Given

A Display implementation writes an integer with Unicode-escaped format braces.
An exact `Temp(21).to_string()` assertion expects `21°C`.

## When

The decoded template changes from `{}` to `{:?}`. Both implementations still
print `21°C` for the pinned integer; the runtime assertion does not discriminate.

## Then

The one changed return-value finding must retain a gap and withhold `exposed`.

## Must Not

Treat Unicode escape syntax as fixed text and borrow credit from the exact pin.
The honesty corpus checks the nonempty real-producer result independently of
helper tests and golden classification expectations.
