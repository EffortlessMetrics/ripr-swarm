<!-- section: Fixed -->
- A verified drop-in `use` item (such as `use pretty_assertions::assert_eq;`)
  emitted by a `macro_rules!` body is read as the standard assertion, as it is
  outside a macro, so such a body no longer refuses the assertion. It is still
  refused when the crate is aliased in `Cargo.toml`, shadowed by a local module,
  renamed or nested (#8149).
