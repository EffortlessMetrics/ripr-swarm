pub fn price(amount: i32) -> i32 {
    amount - 10
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
#[allow(unreachable_code)] // delete with the last todo!()
fn guard_discriminator() -> Result<(), String> {
    let expected: i32 = todo!("ripr: write the value `price` should return for amount = 100");
    if price(100) != expected {
        return Err("price(100) returned the wrong value".into());
    }
    Ok(())
}
#[test]
#[allow(unreachable_code)] // delete with the last todo!()
fn equality_discriminator() {
    let expected: i32 = todo!("ripr: write the value `price` should return for amount = 100");
    assert!(price(100) == expected);
}
}
