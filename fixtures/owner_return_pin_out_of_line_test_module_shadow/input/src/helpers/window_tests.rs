use super::*;

#[test]
fn a_clone_equals_its_original() {
    let window = Window { start: 3, end: 9 };
    assert_eq!(window.clone(), window);
}
