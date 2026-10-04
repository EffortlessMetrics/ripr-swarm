# Fixture: match_arm_expected_side_trap

Spec: RIPR-SPEC-0093

## Given

`flip` swaps two variants, and the diff changes the `Kind::Beta =>` arm:

```rust
match k {
    Kind::Alpha => Kind::Beta,
    Kind::Beta => Kind::Alpha, // changed
}
```

The only test selects the other arm:

```rust
assert_eq!(flip(Kind::Alpha), Kind::Beta);
```

The assertion text contains `Beta`, the changed arm's variant token, but only
on the expected side. Before #5432 that token confirmed the arm and the
finding read `exposed`.

## When

```bash
cargo xtask fixtures match_arm_expected_side_trap
```

## Then

The `Kind::Beta =>` probe reads `weakly_exposed`. Activation names
`Kind::Beta` as the missing discriminator because the test's only owner call
passes `Kind::Alpha`, which selects a different arm, and infection is `weak`
for the same reason.

## Must Not

- Report `exposed` for an arm no test input selects.
- Use mutation-runtime outcome vocabulary.
