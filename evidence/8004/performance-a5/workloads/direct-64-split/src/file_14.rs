mod unit_14 { pub(crate) fn calculate(x: u32) -> u32 { x + 1 }
#[cfg(test)] mod tests { use super::*; #[test] fn subject() { assert_eq!(calculate(6), 7); } } }
