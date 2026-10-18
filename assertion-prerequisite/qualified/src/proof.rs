pub(crate) fn charge(input: bool) -> u64 {
    match input {
        true => 600,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(true), 600); }
}
