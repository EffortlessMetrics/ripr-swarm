mod foreign;
crate::define_scalar!();
fn charge(input: bool) -> u64 {
    match input {
        true => u64(0),
        false => u64(2),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(true), 600); }
}
