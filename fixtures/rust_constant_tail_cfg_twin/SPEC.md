# Fixture: rust_constant_tail_cfg_twin

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #15104

## Given

`#[cfg(not(test))] pub fn ready() -> i32` changes its whole-body tail from
`1` to `2`. A `#[cfg(test)]` twin of `ready` returns `2` and is unchanged.
The unit test asserts `ready() == 2`, so under `cargo test` it calls the
twin and passes on both versions.

## When

`cargo xtask fixtures rust_constant_tail_cfg_twin` runs the public diff
analyzer.

## Then

The return-value finding reads `propagation_unknown`: reach stands, and
propagation is `unknown` with `cfg_gated_owner`, because the gate may compile
the changed body out for the twin the test runs.

## Must Not

- Credit the constant tail's direct-return edge for a `cfg`-gated owner or
  report the finding as `exposed` or `weakly_exposed`.
- Withhold the credit from an ungated owner (see
  `crates/ripr/tests/constant_tail_cfg_twin.rs` controls).
