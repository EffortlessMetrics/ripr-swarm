mod foreign;
struct Input { flag: bool }
fn charge(input: &Input) -> u64 {
    match input.flag {
        true => 600,
        false => 0,
    }
}
#[cfg(test)]
mod tests {
    use super::{charge, Input};
    crate::define_expected!();
    #[test]
    fn observed() { assert_eq!(charge(&Input { flag: true }), u64::MAX); }
}
