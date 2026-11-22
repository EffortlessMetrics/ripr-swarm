# Fixture: rust_custom_lib_path_shaped_call

Spec: RIPR-SPEC-0172

Owner: analysis-fixtures

Issue: #14495

## Given

The package declares `[lib] path = "src/core/engine.rs"`, and `engine.rs`
declares `pub mod pricing;`, so `src/core/pricing.rs` is the module `pricing`
(`my_pkg::pricing::discount`). `discount` changes `amount > 100` to
`amount >= 100`. The only test calls `my_pkg::core::pricing::discount`, the
directory shape, which names no module of this crate.

## When

`cargo xtask fixtures rust_custom_lib_path_shaped_call` runs the public diff
analyzer.

## Then

The finding is not `exposed`: the directory-shaped path does not name the
owner, whose module is read from the declared root.

## Must Not

- Credit the directory-shaped call as naming the owner, or report the finding
  as `exposed`.
- Lose the credit for the real path `my_pkg::pricing::discount` (see
  `crates/ripr/tests/custom_lib_path_module_identity.rs` controls).
