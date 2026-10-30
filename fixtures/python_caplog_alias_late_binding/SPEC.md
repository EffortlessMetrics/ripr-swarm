# Fixture: python_caplog_alias_late_binding

Spec: RIPR-SPEC-0238

## Given

The alias is assigned after its assertion; native Python raises UnboundLocalError before either version can be observed.

## When

The owner warning changes from `low balance: %s` to `balance low: %s`.

## Then

The real classifier reports `static_unknown` with `unsupported_syntax`.

## Must Not

Promote the unestablished snapshot observation to `exposed` (#10595).
