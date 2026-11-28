# Fixture: rust_qualified_pin_neither_version

Spec: RIPR-SPEC-0197

## Given

`pub fn fee(x: u32) -> u32` changes its tail from `x * 2` to `x * 3`, so
`fee(4)` is `8` before and `12` after. The integration test pins the
path-qualified call `rust_qualified_pin_neither_version::fee(4)` to `99`,
which fails on both versions.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: a pin on a value neither version
returns notices nothing, and a module path before the owner's name does not
hide that (#12716, #16400).

## Must Not

- Credit the path-qualified `99` pin as an exposing owner-return pin.
- Withhold credit from a path-qualified pin on the current value `12` (see
  `crates/ripr/tests/invisible_change.rs`).
