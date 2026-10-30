<!-- section: Fixed -->
- Rust: a `#[test_case(.. => literal)]` row now carries its literal inputs into the comparison it stands for, so a row whose inputs cannot tell an edit apart (`0 => 0` against `x * 2` becoming `x * 3`) reads `weakly_exposed` instead of `exposed` (#10797).
