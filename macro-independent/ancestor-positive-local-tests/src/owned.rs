#[derive(Debug)]
struct Dummy;
pub fn charge(input: bool) -> u64 {
    match input {
        true => 600,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn observed() { std::assert_eq!(super::charge(true), 600); }
}
