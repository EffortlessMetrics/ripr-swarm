mod foreign;
fn charge(input: bool) -> u64 {
    match input {
        true => 0,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    use super::charge;
    crate::define_expected!();
    #[test]
    fn observed() { assert_eq!(charge(true), u64::MAX); }
}
