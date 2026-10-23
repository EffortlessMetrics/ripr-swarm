pub fn a() -> u8 {
    1
}

enum Operator<P, Q: Ord + Copy> {
    Prefix(P, Q),
    Postfix(P, Q),
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_is_one() {
        assert_eq!(super::a(), 1);
    }
}
