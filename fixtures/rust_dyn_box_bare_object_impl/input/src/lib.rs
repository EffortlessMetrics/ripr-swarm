pub trait Shape {
    fn len(&self) -> usize;
}

pub struct Square(pub usize);

impl Shape for Square {
    fn len(&self) -> usize {
        self.0 + 1
    }
}

impl Iterator for Shape {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        None
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (4, Some(4))
    }
}

impl ExactSizeIterator for Shape {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_square_len() {
        let s: Box<dyn Shape> = Box::new(Square(3));
        assert_eq!(s.len(), 4);
    }
}
