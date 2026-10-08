# Fixture: owner_return_pin_single_file_raw_rename_shadow

Spec: RIPR-SPEC-0197

## Given

`Window` has a hand-written `impl Clone`, and the single-file `tests`
module (`#[cfg(test)] mod tests`) rebinds the receiver with a
raw-identifier rename (`use crate::other::Gauge as r#Window;`). The test
builds the renamed type through the plain spelling (`Window { .. }`
denotes the same name) and asserts
`assert_eq!(window.clone(), window)`, which runs the derived clone of the
renamed item, never the changed owner. The test module declares no type
of its own, so only the rename check can see the shadow. The test passes
against a wrong clone field and uses nothing from production.

## When

```bash
cargo xtask fixtures owner_return_pin_single_file_raw_rename_shadow
```

or:

```bash
ripr check --root fixtures/owner_return_pin_single_file_raw_rename_shadow/input \
           --diff fixtures/owner_return_pin_single_file_raw_rename_shadow/diff.patch --mode fast
```

## Then

Exactly one field-construction finding reads `weakly_exposed`, never
`exposed`: the single-file rename check refuses the owner-return pin
(RIPR-SPEC-0197 rule 2, #7067), and the struct-field missing discriminator
survives. The related test keeps a name-only relation
(`weak_token_substring`), never `direct_owner_call`: the receiver binds
the renamed item, so no direct production reach is claimed.

## Must Not

- Emit `exposed` for the renamed-receiver clone field.
- Claim `direct_owner_call` reach for the renamed receiver.
- Report zero findings, or drop the struct-field gap.
- Use mutation-runtime outcome vocabulary.
