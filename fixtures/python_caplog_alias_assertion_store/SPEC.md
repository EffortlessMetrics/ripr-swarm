# Fixture: python_caplog_alias_assertion_store

Spec: RIPR-SPEC-0238

## Given

A walrus assignment inside the assertion masks the old/new logging difference; native pytest passes both versions.

## When

The owner warning changes from `low balance: %s` to `balance low: %s`.

## Then

The real classifier reports `static_unknown` with `unsupported_syntax`.

## Must Not

Promote the unestablished snapshot observation to `exposed` (#10595).
