pub fn owner(x: u32) -> u32 {
    3 * x
}
#[cfg(test)]
mod tests {
use super::*;
#[test]
fn pin() {
    assert_eq!(owner(2), 6);
}
}
