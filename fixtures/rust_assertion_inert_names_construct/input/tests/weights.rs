use assertion_inert_names_construct::weight;

fn cases() -> u32 {
    2
}

#[test]
fn weighs_in_a_loop() {
    for _ in 0..cases() {
        assert_eq!(weight(4), 12);
    }
}

#[test]
fn weighs_in_two_shapes() {
    if cases() > 1 {
        assert_eq!(weight(4), 12);
    }
    while cases() > 9 {
        assert_eq!(weight(5), 15);
    }
}
