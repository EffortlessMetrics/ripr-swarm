use super::*;

#[test]
fn render_is_empty() {
    assert_eq!(Sample.render(), String::from(""));
}
