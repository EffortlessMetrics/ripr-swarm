use extlib::Ext;

pub trait Shape {
    fn area(&self) -> u32;
}

pub struct Square(pub u32);

impl Shape for Square {
    fn area(&self) -> u32 {
        self.0 * self.0 + 1
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
