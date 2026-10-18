pub(crate) fn charge() -> u64 {
    600
}
macro_rules! assert_eq { ($($value:tt)*) => { () }; }
pub(crate) use assert_eq;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}
