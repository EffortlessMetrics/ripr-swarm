# Fixture: python_caplog_alias_same_line_mask

Spec: RIPR-SPEC-0238

## Given

A same-line list overwrite masks the old/new logging difference; native pytest passes both versions.

## When

The owner warning changes from `low balance: %s` to `balance low: %s`.

## Then

The real classifier reports `static_unknown` with `unsupported_syntax`.

## Must Not

Promote the unestablished snapshot observation to `exposed` (#10595).
