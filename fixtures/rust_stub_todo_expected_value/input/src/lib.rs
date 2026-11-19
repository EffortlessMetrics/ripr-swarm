pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {
    if amount >= discount_threshold {
        amount - 10
    } else {
        amount
    }
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn below_threshold() { assert_eq!(discounted_total(50, 100), 50); }
#[test]
fn far_above_threshold() { assert_eq!(discounted_total(10_000, 100), 9_990); }
#[test]
#[allow(unreachable_code)] // delete with the last todo!()
fn boundary_discriminator() {
    let amount: i32 = 100;
    let threshold: i32 = 100;
    let expected: i32 = todo!("ripr: write the value `discounted_total` should return for amount = 100, threshold = 100");
    assert_eq!(discounted_total(amount, threshold), expected);
}
}
