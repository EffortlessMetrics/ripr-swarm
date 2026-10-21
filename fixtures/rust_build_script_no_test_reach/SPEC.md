# Fixture: rust_build_script_no_test_reach

Spec: RIPR-SPEC-0172

## Given

A Rust crate whose root `build.rs` gains `.arg("--cap-lints=allow")` in
`compile_probe`. An unrelated integration test is named `test_allow_attributes`
and never runs the build script.

## When

```bash
cargo xtask fixtures rust_build_script_no_test_reach
```

or:

```bash
ripr check --root fixtures/rust_build_script_no_test_reach/input --diff fixtures/rust_build_script_no_test_reach/diff.patch --mode fast
```

## Then

The finding relates no test. Cargo runs a build script before the package
compiles, in a separate crate, so no test of the package can call it. The token
`allow` from the string literal does not make `test_allow_attributes` a related
test, and the finding is not `reachable_unrevealed`.

## Must Not

- Report `reachable_unrevealed` or `reach weak` for a root build script.
- Tell the reader to add an assertion to an unrelated test.
- Relate a test to a build script through a name, stem or substring token.
