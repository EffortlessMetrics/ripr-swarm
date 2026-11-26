pub trait Stock {
    fn count(&self) -> usize;
}
pub struct Shelf {
    left: usize,
}
pub struct Bin(pub usize);
impl Shelf {
    pub fn stocked(left: usize) -> Shelf {
        Shelf { left }
    }
}
impl Iterator for Shelf {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        Some(self.left)
    }
}
impl Stock for Shelf {
    fn count(&self) -> usize {
        self.left + 1
    }
}
impl Stock for Bin {
    fn count(&self) -> usize {
        self.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shelf_count() {
        assert_eq!(Shelf::stocked(3).count(), 3);
    }
}
