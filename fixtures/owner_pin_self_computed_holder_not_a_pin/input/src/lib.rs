pub fn weight(x: u32) -> u32 {
    x * 3
}

#[cfg(test)]
mod tests {
    use super::weight;

    // A helper macro anywhere in the file, and a cfg-gated holder shadowed by
    // a plain same-named type: neither makes the expected value independent.
    macro_rules! unused_helper {
        () => {};
    }

    #[cfg(any())]
    struct Counter(std::sync::atomic::AtomicUsize);
    #[allow(dead_code)]
    struct Counter(u32);

    #[test]
    fn expected_computed_by_the_owner_is_not_independent() {
        let expected: u32 = weight(4);
        assert_eq!(weight(4), expected);
    }

    #[test]
    fn expected_computed_by_the_owner_with_an_inferred_type_is_not_independent() {
        let expected = weight(4);
        assert_eq!(weight(4), expected);
    }
}
