# Starter examples

Three small projects you can run `ripr` on before pointing it at your own
repository. Each one has the same story:

1. A change moves a discount boundary from exclusive (`>`) to inclusive (`>=`).
2. The tests check a value on each side of the boundary, but not the boundary
   itself, so they pass before and after the change.
3. `ripr check` reports the changed comparison as weakly exposed and names the
   missing test input.
4. One added assertion at the boundary moves the finding to exposed.

| Example | Language | Status |
| --- | --- | --- |
| [rust-discount-threshold](rust-discount-threshold/README.md) | Rust | supported |
| [python-bulk-discount](python-bulk-discount/README.md) | Python | preview, advisory |
| [typescript-discount-threshold](typescript-discount-threshold/README.md) | TypeScript | preview, advisory |

Every example has the same layout:

```text
change.diff      the change under review, relative to project/
project/         the project after the change, with its tests as first written
strengthened/    the test file with the one added boundary assertion
```

The results are static evidence about what the tests assert. `ripr` does not
run the tests or any mutant, so `exposed` means a test appears to check the
changed behavior exactly, not that a mutation run was performed.

`crates/ripr/tests/starter_examples.rs` runs the commands below on every
example and checks the finding before and after, so these pages stay true.

To use the library API instead of the CLI, see the Rust examples in
[`crates/ripr/examples`](../crates/ripr/examples).
