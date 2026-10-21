# Fixture: rust_path_keyword_affinity

Spec: RIPR-SPEC-0172

## Given

A Rust crate whose only changed line is an owner-less import,
`pub(crate) use std::collections::BTreeMap` to `BTreeSet`. An integration test
in the same package asserts on `crate::helper::pick(1)`, a different function
that shares only the `crate` path keyword with the changed line.

## When

```bash
cargo xtask fixtures rust_path_keyword_affinity
```

or:

```bash
ripr check --root fixtures/rust_path_keyword_affinity/input --diff fixtures/rust_path_keyword_affinity/diff.patch --mode fast
```

## Then

The finding relates no test: `crate` is syntax, not a name for the changed
behavior, so it does not seed assertion-target affinity. The test's
`assert_eq!` is not credited as an oracle for the import.

## Must Not

- Report `reach yes`, `observation yes` or a related test for the unrelated
  `picks_two` test.
- Credit the `crate::helper::pick(1)` exact-value oracle to the import.
- Treat a path or declaration keyword (`crate`, `super`, `const`, `static`) as an
  identifier that names the changed behavior.
