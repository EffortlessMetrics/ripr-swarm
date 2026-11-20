use crate::Shape;

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
