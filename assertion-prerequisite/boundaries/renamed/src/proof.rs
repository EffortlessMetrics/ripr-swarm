pub(crate) fn charge() -> u64 {
    600
}
use core::assert_eq as println;
fn unused() { println!(1, 1); }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
