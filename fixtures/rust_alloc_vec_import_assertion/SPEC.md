# Fixture: rust_alloc_vec_import_assertion

Spec: RIPR-SPEC-0197

Issue: #15263

## Given

The textwrap `src/columns.rs` shape: a `no_std`-style test module declares
`extern crate alloc;` and imports the standard macro by path with
`use alloc::vec;`. Production `widen` changes its guard from `n > 10` to
`n >= 10`. The only test is a plain, always-run `assert_eq!(widen(1), vec![1, 2]);`.

## When

```bash
cargo xtask fixtures rust_alloc_vec_import_assertion
```

## Then

The import names the standard `vec!`, so the `assert_eq!` keeps its credit and
the finding grades the same as with no import. No related-test row reads
"imports a different `vec` by name", and the next step does not ask the
developer to rewrite the assertion as the standard `assert_eq!`.

## Must Not

- Report the `use alloc::vec;` import as a rebinding of `vec!`.
- Claim the boundary is discriminated: the test input 1 does not reach the
  changed `n == 10` boundary.
