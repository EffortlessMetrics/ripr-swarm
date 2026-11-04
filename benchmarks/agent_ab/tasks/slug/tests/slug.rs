use ab_slug::slugify;

#[test]
fn slugify_produces_output() {
    assert!(!slugify("Hello, World").is_empty());
}
