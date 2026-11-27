use std::ops::Deref;

pub struct Inner {
    pub cap: usize,
}

mod inherent {
    use super::*;

    impl Inner {
        fn len(&self) -> usize {
            self.cap * 2 + 1
        }
    }
}

impl Iterator for Inner {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (7, Some(7))
    }
}

impl ExactSizeIterator for Inner {}

pub struct Guard(pub Inner);

impl Deref for Guard {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_len() {
        let g = Guard(Inner { cap: 3 });
        assert_eq!(g.len(), 7);
    }
}
