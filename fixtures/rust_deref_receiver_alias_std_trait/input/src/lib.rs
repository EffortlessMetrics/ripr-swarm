use std::ops::Deref;

pub struct Inner {
    pub cap: u32,
}

impl Inner {
    pub fn len(&self) -> u32 {
        self.cap * 2 + 1
    }
}

pub struct Guard(pub Inner);

pub type G = Guard;

impl Iterator for G {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}

impl ExactSizeIterator for G {}

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
    fn guard_limit() {
        let g = Guard(Inner { cap: 3 });
        assert_eq!(g.len(), 0);
    }
}
