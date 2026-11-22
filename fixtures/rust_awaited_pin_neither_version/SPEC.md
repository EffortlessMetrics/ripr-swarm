# Fixture: rust_awaited_pin_neither_version

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #15397

## Given

`pub async fn fee(x: u32) -> Result<u32, String>` changes its tail from
`Ok(x * 2)` to `Ok(x * 3)`, so `fee(4)` is `Ok(8)` before and `Ok(12)` after.
Two `#[tokio::test]` integration tests pin the awaited call to `Ok(99)`, one
with `assert!(matches!(fee(4).await, Ok(99)))` and one with
`assert_eq!(fee(4).await, Ok(99))`. Both fail on both versions.

## When

`cargo xtask fixtures rust_awaited_pin_neither_version` runs the public diff
analyzer.

## Then

The return-value finding is not `exposed`: a pin on a value neither version
returns notices nothing, and `.await` does not hide that (#12716).

## Must Not

- Credit either awaited `Ok(99)` assertion as an exposing owner-return pin.
- Withhold credit from an awaited pin on the baseline value (see
  `crates/ripr/tests/awaited_matches_pin.rs` controls).
