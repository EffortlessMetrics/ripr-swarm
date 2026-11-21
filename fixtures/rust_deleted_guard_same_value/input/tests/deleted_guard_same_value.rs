use deleted_guard_same_value::*;

#[test]
fn pick_pins_the_guard_input() {
    assert_eq!(pick(0), Vec::<u32>::new());
}

#[test]
fn pick_nine_pins_the_guard_input() {
    assert_eq!(pick_nine(0), Vec::<u32>::new());
}
