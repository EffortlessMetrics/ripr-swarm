pub fn make(input: u8) -> u8 {
    input + 2
}

pub fn other(x: u8) -> u8 {
    x + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_smoke() {
        let _made = make(3);
    }

    #[test]
    fn other_test() {
        assert_eq!(other(1), 2);
    }
}
