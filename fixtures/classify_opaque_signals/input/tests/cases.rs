use classify_opaque_signals_fixture::*;

#[test]
fn test_case_one() { let fixture = 10; assert!(case_one(fixture)); }
#[test]
fn test_case_two() { let builder = 10; assert!(case_two(builder)); }
#[test]
fn test_case_three() { let arb_value = 10; assert!(case_three(arb_value)); }
#[test]
fn test_case_four() { assert!(case_four(10)); }
#[test]
fn test_case_five() { assert!(case_five(10)); }
#[test]
fn test_case_six() { assert!(case_six(10)); }
#[test]
fn test_case_seven() { assert!(case_seven(10)); }
