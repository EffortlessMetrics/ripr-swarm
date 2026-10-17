#[test]
fn renders() {
    std::fs::write("tests/w.txt", written_expected::render(4).to_string()).unwrap();
    let want: String = std::fs::read_to_string("tests/w.txt").unwrap();
    assert_eq!(written_expected::render(4), want.trim().parse::<i32>().unwrap());
}
