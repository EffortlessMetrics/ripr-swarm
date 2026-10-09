use owner_pin_control::weight;

#[test]
fn checks_weight() -> Result<(), String> {
    if weight(4) != 12 {
        return Err(format!("weight(4) was {}", weight(4)));
    }
    Ok(())
}
