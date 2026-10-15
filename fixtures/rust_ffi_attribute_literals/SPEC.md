# Fixture: rust_ffi_attribute_literals

Spec: RIPR-SPEC-0062

Owner: analysis-fixtures

Issue: [#8090](https://github.com/EffortlessMetrics/ripr-swarm/issues/8090)

## Given

Three Rust functions change the same threshold. `ordinary` has a raw doc
string containing `, no_mangle,` between embedded quotes. `exported` has a
real `unsafe(no_mangle)` attribute after a raw doc string with an unmatched
quote and opening parenthesis in its contents. `commented` has the binding
name inside a block comment. All three sources compile on Rust 2024 without
dependencies, and no Rust tests exist.

`attribute-cases.json` adds 33 parser-valid controls for raw strings, predicate
splitting, nested and line comments, wrapper trivia, raw identifiers and
case-sensitive marker names. Each case creates one changed predicate and checks
the public finding's limitation. The `compile` flag marks 26 standalone cases
whose compiler symbol identities were independently checked with Rust 1.95;
the integration test itself does not invoke a compiler. Remaining cases check
syntactic binding hints or refusals, without resolving external attribute macros
or evaluating `cfg_attr` predicates.

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

Every matrix source must parse without errors and produce exactly one
`no_static_path` finding before its literal expected limitation is checked.
Raw identifiers keep their case-sensitive Rust identity. `r#cfg_attr` and
`unsafe(r#no_mangle)` retain the binding, while `r#unsafe` names an ordinary
attribute rather than the special unsafe-attribute keyword wrapper.

## Must Not

- Invent a binding from raw-string or comment contents.
- Lose a real attribute because raw-string contents contain quotes or delimiters.
- Promote any finding to exposed or infer that an external test exists.
- Recognize a path segment inside a comment, lowercase an unrelated attribute
  name, or treat an unknown wrapper's arguments as binding attributes.
