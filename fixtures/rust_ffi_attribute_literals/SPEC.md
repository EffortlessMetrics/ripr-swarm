# Fixture: rust_ffi_attribute_literals

Spec: RIPR-SPEC-0062

Owner: analysis-fixtures

## Given

Three Rust functions change the same threshold. `ordinary` has a raw doc
string containing `, no_mangle,` between embedded quotes. `exported` has a
real `unsafe(no_mangle)` attribute after a raw doc string with an unmatched
quote and opening parenthesis in its contents. `commented` has the binding
name inside a block comment. All three sources compile on Rust 2024 without
dependencies, and no Rust tests exist.

## When

```bash
cargo xtask fixtures rust_ffi_attribute_literals
cargo test -p ripr --test ffi_attribute_literals
```

The public diff analysis examines the predicates at `src/lib.rs:3`,
`src/lib.rs:8` and `src/lib.rs:13`.

## Then

All three findings remain `no_static_path`. Only `exported` carries
`cross_language_oracle_visibility_unresolved` and guidance to verify the
external-language oracle. The ordinary and commented functions retain
same-language test guidance.

## Must Not

- Invent a binding from raw-string or comment contents.
- Lose a real attribute because raw-string contents contain quotes or delimiters.
- Promote any finding to exposed or infer that an external test exists.
