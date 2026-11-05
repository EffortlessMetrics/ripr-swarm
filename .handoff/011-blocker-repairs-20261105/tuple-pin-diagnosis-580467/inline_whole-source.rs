pub struct WhileSome {
    pub remaining: usize,
}

pub struct Combinations {
    pub remaining: usize,
}

impl Iterator for WhileSome {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.remaining -= 1;
            Some(self.remaining)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.remaining))
    }
}

impl Iterator for Combinations {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.remaining -= 1;
            Some(self.remaining)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

#[cfg(test)]
mod tests {
use super::*;

#[test]
fn while_some_size_hint_upper_bound() {
    let it = WhileSome { remaining: 2 };
    assert_eq!(it.size_hint(), (0, Some(it.remaining)));
}

}
