pub fn gate(n: u32) -> bool {
    n >= 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(want: bool, got: bool) {
        assert_eq!(got, want);
    }

    #[test]
    fn boundary() {
        run_test(true, gate(10).clone());
    }
}
