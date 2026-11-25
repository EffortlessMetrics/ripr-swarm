pub fn widen(n: usize) -> Vec<usize> {
    if n >= 10 {
        return Vec::new();
    }
    vec![n, n + 1]
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;

    #[test]
    fn widens() {
        assert_eq!(widen(1), vec![1, 2]);
    }
}
