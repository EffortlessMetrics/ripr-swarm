// Stands in for `approx::assert_relative_ne!`; the fixture has no registry
// dependency. ripr must not read the spelling as an oracle, and must not
// advise tightening the tolerance: this form passes when the values differ by
// more than the tolerance. The body panics on purpose: an `assert!` body lets
// ripr expand the macro and changes the grade.
macro_rules! assert_relative_ne {
    ($actual:expr, $other:expr, epsilon = $epsilon:expr) => {
        if ($actual - $other).abs() <= $epsilon { panic!("within tolerance"); }
    };
}

#[test]
fn test_scale_differs_from_the_factor() {
    let result = rust_tolerance_ne_assertion_macro_limitation_fixture::scale(10.0, 3.0);
    assert_relative_ne!(result, 3.0, epsilon = 1e-9);
}
