use std::cell::Cell;

pub struct Wrapper {
    inner: Cell<u32>,
}

impl Wrapper {
    pub fn get(&self) -> u32 {
        if self.inner.get() > 10 {
            10
        } else {
            self.inner.get()
        }
    }
}

pub fn other(x: u32) -> u32 {
    x.min(5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_test() {
        assert_eq!(other(9), 5);
    }
}
