pub(crate) fn charge() -> u64 {
    0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
