# Fixture: infect_value_returned

Spec: RIPR-SPEC-0094

## Given

Production code changes so that a function result is stored in a named
binding `let result = helper(amount * 9)` which is subsequently used in a
tail expression `result + 1`.  The changed value IS read into the returned
value — this is a genuine infecting change.

A test with an exact-value assertion exercises `score`:

```rust
#[test]
fn score_returns_expected_value() {
    assert_eq!(score(10), 10);
}
```

## When

```bash
cargo xtask fixtures infect_value_returned
```

or:

```bash
ripr check --root fixtures/infect_value_returned/input --diff fixtures/infect_value_returned/diff.patch --mode fast
```

## Then

`ripr` must emit `exposed` for the named binding probe.  A named binding
(`let result = …`) is NOT a discard — the value flows into `result + 1`.
This is the **control** for fix A in
[RIPR-SPEC-0094](../../docs/specs/RIPR-SPEC-0094-infect-propagate-fail-closed.md):
the wildcard-discard predicate must NOT fire for `let _name = …`.

## Must Not

- Classify `let result = helper(…)` as `infection_unknown`.
- Downgrade any finding whose named binding is used in a subsequent expression.
- Use mutation-runtime outcome vocabulary.
