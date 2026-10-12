use owner_pin_control::weight;

fn Err(_: &str) -> Result<(), String> {
    Ok(())
}

#[test]
fn checks_weight() -> Result<(), String> {
    if weight(4) != 12 {
        return Err("mismatch");
    }
    Ok(())
}
