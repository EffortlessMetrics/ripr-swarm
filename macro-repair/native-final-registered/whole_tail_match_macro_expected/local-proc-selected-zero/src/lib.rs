mod foreign;
struct Input { flag: bool }
fn charge(input: &Input) -> u64 {
    match input.flag {
        true => 0,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    use super::{charge, Input};
    #[test]
    fn observed() { #[opaque::make_expected] fn marker() {} assert_eq!(charge(&Input { flag: true }), u64::MAX); }
}
