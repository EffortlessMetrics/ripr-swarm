#[derive(Debug)]
struct Dummy;
pub fn charge(input: bool) -> u64 {
    match input {
        true => u64(600),
        false => u64(2),
    }
}
