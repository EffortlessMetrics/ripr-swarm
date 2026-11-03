<!-- section: Fixed -->
- Treat bare `cfg!`, `column!`, `file!`, `line!`, and `module_path!` as
  helper-chain-transparent when no indexed file rebinds those names, restoring
  forwarding credit through wrappers that only read compile-time literals
  (#10357 item 3). `env!`, path-qualified macros, and non-`TRUSTED_MACRO_NAMES`
  names remain opaque.
