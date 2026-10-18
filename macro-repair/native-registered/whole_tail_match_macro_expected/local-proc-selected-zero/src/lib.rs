mod foreign;
fn charge(input: bool) -> u64 {
    match input {
        true => 0,
        false => 2,
    }
}
fn input() -> bool { true }
#[cfg(test)]
mod tests {
    use super::{charge, input};
    #[test]
    fn observed() { #[opaque::make_expected] fn marker() {} assert_eq!(charge(input()), u64::MAX); }
}
