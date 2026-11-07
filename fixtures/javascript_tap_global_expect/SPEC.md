# Fixture: javascript_tap_global_expect

Spec: RIPR-SPEC-0085
Spec: RIPR-SPEC-0108

## Given

A tap file spells global Jest-style `expect` assertions without importing
that binding. Native tap supplies no such global: both the original and
changed implementations fail with `expect is not defined`.

## When

```bash
cargo xtask fixtures javascript_tap_global_expect
```

## Then

Both tests remain related with unknown oracle evidence. The finding stays
`weakly_exposed` and emits no repair packet.

## Must Not

- Borrow Jest's ambient assertion authority for a tap registration.
- Treat the independent native reference error as discrimination of the edit.
