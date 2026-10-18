pub(crate) fn charge() -> u64 {
    600
}
fn unused() { core::assert_eq!(1, 1); }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
