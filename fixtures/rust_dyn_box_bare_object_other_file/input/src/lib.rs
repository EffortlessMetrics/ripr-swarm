pub trait Shape {
    fn len(&self) -> usize;
}

pub struct Square(pub usize);

impl Shape for Square {
    fn len(&self) -> usize {
        self.0 + 1
    }
}

mod objects;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_square_len() {
        let s: Box<dyn Shape> = Box::new(Square(3));
        assert_eq!(s.len(), 4);
    }
}
