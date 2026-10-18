#[derive(Debug)]
struct Dummy;
pub fn charge(input: bool) -> u64 {
    match input {
        true => 600,
        false => 2,
    }
}
