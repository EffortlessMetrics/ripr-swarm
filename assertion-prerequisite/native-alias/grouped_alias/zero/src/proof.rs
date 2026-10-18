pub(crate) fn charge() -> u64 {
    0
}
pub(crate) use opaque as external_alias;
#[cfg(test)]
mod tests {
    use super::*;
    use super::external_alias::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
