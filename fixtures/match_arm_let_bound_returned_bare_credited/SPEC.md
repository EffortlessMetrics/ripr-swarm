# Fixture: match_arm_let_bound_returned_bare_credited

Spec: RIPR-SPEC-0229

## Given

`bucket` binds the `match` to a local and returns that local unchanged, so the
arm's value is the owner's return value. The fallback arm changes from `10` to
`20`, and the test pins it:

```rust
assert_eq!(bucket("q"), 20);
```

## When

```bash
cargo xtask fixtures match_arm_let_bound_returned_bare_credited
```

## Then

The `match_arm` finding stays `exposed`. A `let` binding returned bare does not
remap the arm's result.

## Must Not

- Withhold arm credit just because the `match` is bound to a local first.

Refs #12143.
