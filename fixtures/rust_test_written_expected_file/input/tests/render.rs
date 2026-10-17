#[test]
fn renders() {
    std::fs::write("tests/w.txt", written_expected::render(4).to_string()).unwrap();
    assert_eq!(written_expected::render(4), std::fs::read_to_string("tests/w.txt").unwrap().trim().parse::<i32>().unwrap());
}
