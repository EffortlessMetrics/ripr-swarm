crate::test_flags!();

#[test]
fn cases() {
    let value: Vec<(&str, TestFlags)> = vec![("A", TestFlags::A), ("B", TestFlags::B)];
    for (name, _) in &value {
        let _ = name;
    }
    assert!(value.contains(&("A", TestFlags::A)));
}
