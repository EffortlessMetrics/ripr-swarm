# Fixture: owner_return_pin_test_module_shadow

Spec: RIPR-SPEC-0197

## Given

`Window` has a hand-written `impl Clone`, and the `mod tests` in the same
file declares its own same-name `Window` with a derived `Clone`. The test
builds the test-local type and asserts
`assert_eq!(window.clone(), window)`, which runs the derived clone, never
the changed owner. The test passes against a wrong clone field and uses
nothing from production (the `use super::*` import is unused).

## When

```bash
cargo xtask fixtures owner_return_pin_test_module_shadow
```

or:

```bash
ripr check --root fixtures/owner_return_pin_test_module_shadow/input \
           --diff fixtures/owner_return_pin_test_module_shadow/diff.patch --mode fast
```

## Then

Exactly one field-construction finding reads `weakly_exposed`, never
`exposed`: the test-module shadow refuses the owner-return pin
(RIPR-SPEC-0197 rule 2, #6905), and the struct-field missing discriminator
survives.

## Must Not

- Emit `exposed` for the shadowed clone field.
- Report zero findings, or drop the struct-field gap.
- Use mutation-runtime outcome vocabulary.
