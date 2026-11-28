use super::*;

#[test]
fn render_is_empty() {
    assert_eq!((-0.0f64).render(), String::from(""));
}
