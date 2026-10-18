mod foreign;
fn charge(input: bool) -> u64 {
    match input {
        true => 600,
        false => 2,
    }
}
fn input() -> bool { true }
#[cfg(test)]
mod tests {
    use super::charge;
    crate::define_expected!();
    #[test]
    fn observed() { assert_eq!(charge(input()), u64::MAX); }
}
