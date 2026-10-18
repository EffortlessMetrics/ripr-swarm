pub(crate) fn charge() -> u64 {
    0
}
use opaque::emit_exported as println;
fn install() { println!($); }
#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
