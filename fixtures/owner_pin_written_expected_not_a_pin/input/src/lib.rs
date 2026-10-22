pub fn weight(x: u32) -> u32 {
    x * 3
}

#[cfg(test)]
mod tests {
    use super::weight;
    use std::sync::atomic::{AtomicU32, Ordering};

    static VALUE: AtomicU32 = AtomicU32::new(0);

    struct E {
        _tag: u32,
    }

    impl std::ops::Deref for E {
        type Target = AtomicU32;
        fn deref(&self) -> &Self::Target {
            &VALUE
        }
    }

    // A struct of literals can Deref to an atomic the owner result is stored in.
    #[test]
    fn deref_store_is_not_independent() {
        let expected = E { _tag: 0 };
        AtomicU32::store(&expected, weight(4), Ordering::SeqCst);
        assert_eq!(weight(4), expected.load(Ordering::SeqCst));
    }

    // A keyword before `[` starts an array destructuring assignment.
    #[test]
    fn match_destructuring_is_not_independent() {
        let mut expected = 0;
        let mut other = 0;
        match [expected, other] = [weight(4), 0] {
            _ => {}
        }
        let _ = other;
        assert_eq!(weight(4), expected);
    }
}
