use match_arm_expected_side_trap_fixture::{Kind, flip};

#[test]
fn alpha_flips_to_beta() {
    assert_eq!(flip(Kind::Alpha), Kind::Beta);
}
