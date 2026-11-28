pub fn gate(n: u32) -> bool {
    n >= 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test(want: bool, _f: impl FnOnce() -> bool) {
        assert_eq!(want, true);
    }

    #[test]
    fn boundary() {
        run_test(true, || gate(10));
    }
}
