# Fixture: rust_path_keyword_affinity

Spec: RIPR-SPEC-0172

## Given

A Rust crate whose changed lines are owner-less: an import
(`pub(crate) use std::collections::BTreeMap` to `BTreeSet`), a `pub union`
field type, a `macro_rules!` body and a `SEED_BASE` constant. Integration tests
in the same package assert on `crate::helper::pick(1)`, on a block declaring
its own `union OtherItem`, and on a block declaring its own `macro_rules!
other_item`: different code that shares only a path or declaration keyword with
the changed lines. A further test asserts on `SEED_BASE`, a real identifier of
the changed constant.

## When

```bash
cargo xtask fixtures rust_path_keyword_affinity
```

or:

```bash
ripr check --root fixtures/rust_path_keyword_affinity/input --diff fixtures/rust_path_keyword_affinity/diff.patch --mode fast
```

## Then

The import, union and macro findings relate no test: `crate`, `union` and
`macro_rules` as declaration introducers are syntax, not names for the changed
behavior, so they do not seed assertion-target affinity. The unrelated
`assert_eq!`s are not credited as oracles. The `SEED_BASE` constant still
relates the `seeds_base` test through the shared identifier.

## Must Not

- Report `reach yes`, `observation yes` or a related test for the unrelated
  `picks_two`, `checks_union_result` or `checks_macro_result` tests.
- Credit the `crate::helper::pick(1)` exact-value oracle to the import.
- Treat a path or declaration keyword (`crate`, `super`, `const`, `static`,
  a declaring `union`, `macro_rules!`) as an identifier that names the changed
  behavior.
