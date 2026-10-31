# Fixture: rust_display_raw_placeholder_edit

Spec: RIPR-SPEC-0297

## Given

A Display tail has a raw template containing internal quotes and an exact
assertion of its complete output, including the quoted prefix.

## When

The template changes from `{}` to `{:?}`. The pinned integer 21 prints the
same complete string under both templates.

## Then

The one return-value finding retains a gap. Raw templates are outside the
bounded literal-only admission, including through generic reveal fallbacks.

## Must Not

Borrow token overlap between the unchanged quoted prefix and the expected
value to recover `exposed` after the format edit was refused.
