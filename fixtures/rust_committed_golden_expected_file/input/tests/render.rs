#[test]
fn renders() {
    assert_eq!(written_expected::render(4), std::fs::read_to_string("tests/w.txt").unwrap().trim().parse::<i32>().unwrap());
}
