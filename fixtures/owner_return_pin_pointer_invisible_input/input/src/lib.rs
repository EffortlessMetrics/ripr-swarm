pub fn capped(n: u32) -> u32 {
    n.min(6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_both_caps() {
        let f: fn(u32) -> u32 = capped;
        assert_eq!(f(3), 3);
    }
}
