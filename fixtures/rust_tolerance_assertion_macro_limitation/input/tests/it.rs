// Stands in for `approx::assert_relative_eq!`; the fixture has no registry
// dependency. ripr must not read the spelling as an oracle.
macro_rules! assert_relative_eq {
    ($actual:expr, $expected:expr, epsilon = $epsilon:expr) => {
        if ($actual - $expected).abs() > $epsilon { panic!("mismatch"); }
    };
}

#[test]
fn test_scale_within_tolerance() {
    let result = rust_tolerance_assertion_macro_limitation_fixture::scale(10.0, 3.0);
    assert_relative_eq!(result, 30.0, epsilon = 1e-9);
}
