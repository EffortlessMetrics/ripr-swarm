use guard_operand_wrapper_agreement::is_pow2;

fn check(actual: String, expected: String) {
    assert_eq!(actual, expected);
}

#[test]
fn rendered_power() {
    let actual = format!("{}", is_pow2(4));
    let expected = String::from("true");
    check(actual, expected);
}
