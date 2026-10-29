# Fixture: rust_binary_alias_foreign_dependency

Spec: RIPR-SPEC-0153

## Given

A binary imports `my_macros` from a registry dependency. An indexed workspace
proc-macro package has the same name and defines `wrapped_test`, but the
manifest does not bind the binary to that package.

## When

`ripr check` analyzes the binary's changed return value.

## Then

The finding has no related tests and remains `no_static_path`.

## Must Not

Treat the workspace namesake as evidence that the registry dependency's alias
is a test wrapper, or promote the finding on that basis.
