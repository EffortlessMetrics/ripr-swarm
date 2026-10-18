mod types;
use types::*;
fn charge(x: bool) -> u64 {
    match x {
        true => u64(1),
        false => u64(2),
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn pins() { assert_eq!(charge(true), 600); }
}
