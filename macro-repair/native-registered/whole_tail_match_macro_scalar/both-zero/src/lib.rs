mod foreign;
crate::define_scalar!();
fn charge(input: bool) -> u64 {
    match input {
        true => u64(0),
        false => u64(0),
    }
}
fn input() -> bool { true }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(input()), 600); }
}
