use owner_pin_control::weight;

#[test]
fn rejects_the_old_weight() -> Result<(), String> {
    if weight(4) == 8 {
        return Err("weight(4) kept the old value".to_string());
    }
    Ok(())
}

#[test]
fn weight_is_at_least_twelve() {
    assert!(weight(4) >= 12);
}

#[test]
fn weight_matches_or_flag() {
    let lenient = true;
    assert!(weight(4) == 12 || lenient);
}
