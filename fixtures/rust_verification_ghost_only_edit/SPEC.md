# Fixture: rust_verification_ghost_only_edit

Spec: RIPR-SPEC-0368

## Given

A Creusot crate whose diff edits only a `#[logic]` function body and a
`proof_assert!` inside a runtime function. Creusot erases both from an ordinary
build; the runtime body `x + 1` and its test are untouched.

## When

```bash
cargo xtask fixtures rust_verification_ghost_only_edit
```

or:

```bash
ripr check --root fixtures/rust_verification_ghost_only_edit/input --diff fixtures/rust_verification_ghost_only_edit/diff.patch --mode fast
```

## Then

`ripr` should report `no_behavioral_candidates` with no findings and one
`verification_only_change` limitation. No actionable gap and no
"add a co-located test" start appears for either edited line.

## Must Not

- Seed a probe on the `#[logic]` body or the `proof_assert!` line.
- Offer a test-gap repair for text no test can reach or observe.
- Treat the ghost text as an oracle or as a discriminator for `bump`.
