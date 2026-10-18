pub(crate) fn charge() -> u64 {
    600
}
macro_rules! opaque { () => { struct Extra; }; }
opaque!();
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
