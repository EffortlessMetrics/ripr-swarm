# Fixture: python_literal_value_inside_longer_number

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

A multi-line dict literal changes its `lo` value to `80`.
The only test looks for `8080` in the dict's repr and passes before and after the edit.

## When

The single changed element line is analyzed.

## Then

One `weakly_exposed` finding withholds credit: `80` appears only inside the longer number `8080`.

## Must Not

Credit a changed number found as a substring of a longer number.
The honesty corpus independently rejects promotion after an incorrect re-bless.
