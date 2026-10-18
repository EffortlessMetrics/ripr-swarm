mod foreign;
fn charge(input: bool) -> u64 {
    match input {
        true => 600,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    use super::charge;
    #[test]
    fn observed() { crate::define_expected!(); assert_eq!(charge(true), u64::MAX); }
}
