# Fixture: owner_return_pin_out_of_line_test_module_shadow

Spec: RIPR-SPEC-0197

## Given

`Window` has a hand-written `impl Clone`, and the out-of-line `helpers`
test module (`src/helpers.rs`, declared by `#[cfg(test)] mod helpers;`)
declares its own same-name `Window` with a derived `Clone`. The nested
child test (`src/helpers/window_tests.rs`) builds the test-local type and
asserts `assert_eq!(window.clone(), window)`, which runs the derived
clone, never the changed owner. The child file itself declares nothing,
so the single-file shadow prefilter is blind. The test passes against a
wrong clone field and uses nothing from production.

## When

```bash
cargo xtask fixtures owner_return_pin_out_of_line_test_module_shadow
```

or:

```bash
ripr check --root fixtures/owner_return_pin_out_of_line_test_module_shadow/input \
           --diff fixtures/owner_return_pin_out_of_line_test_module_shadow/diff.patch --mode fast
```

## Then

Exactly one field-construction finding reads `weakly_exposed`, never
`exposed`: the parent-chain shadow refuses the owner-return pin
(RIPR-SPEC-0197 rule 2, #6950), and the struct-field missing discriminator
survives. The related test keeps a name-only relation
(`weak_token_substring`), never `direct_owner_call`: the receiver binds
the test-local shadow, so no direct production reach is claimed.

## Must Not

- Emit `exposed` for the shadowed clone field.
- Claim `direct_owner_call` reach for the shadowed receiver.
- Report zero findings, or drop the struct-field gap.
- Use mutation-runtime outcome vocabulary.
