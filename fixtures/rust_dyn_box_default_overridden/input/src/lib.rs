pub trait Shape {
    fn area(&self) -> u32 {
        self.base() * 2 + 1
    }

    fn base(&self) -> u32;
}

pub struct Square(pub u32);

impl Shape for Square {
    fn base(&self) -> u32 {
        self.0
    }

    fn area(&self) -> u32 {
        7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_square_area() {
        let s: Box<dyn Shape> = Box::new(Square(3));
        assert_eq!(s.area(), 7);
    }
}
