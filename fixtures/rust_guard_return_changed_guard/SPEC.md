# Fixture: rust_guard_return_changed_guard

Spec: RIPR-SPEC-0197

## Given

Production code changes an early-return guard's condition and its returned
value in one edit:

```rust
if t > 500 {
    return t / 10;
}
```

to:

```rust
if t > 200 {
    return t / 5;
}
```

The only test asserts `rate(300) == 60`. That is 60 on both real versions
(the old tail `300 * 2 / 10`, the new guard `300 / 5`), so it cannot notice
the change.

## When

```bash
cargo xtask fixtures rust_guard_return_changed_guard
```

or:

```bash
ripr check --root fixtures/rust_guard_return_changed_guard/input --diff fixtures/rust_guard_return_changed_guard/diff.patch --mode draft
```

## Then

The changed `return t / 5` stays `weakly_exposed`. The guard-return closed pin
(#14387) credits a pin only when the return line is the owner's only changed
line, because the old owner rebuilt from the return line alone (`t > 200`,
`t / 10`, giving 30) is not the real old owner. Without that check the pin
would read `exposed` (a false promotion, #14786 review).

## Must Not

- Credit `rate(300) == 60` as an exact pin on the changed `return t / 5`: the
  value is the same on both real versions.
- Rebuild the old owner from the return line alone when the guard condition
  changed in the same edit.
