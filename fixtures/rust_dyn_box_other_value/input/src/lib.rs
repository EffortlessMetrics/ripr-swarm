pub trait Shape {
    fn area(&self) -> u32;
}

pub struct Square(pub u32);

impl Shape for Square {
    fn area(&self) -> u32 {
        self.0 * self.0 + 1
    }
}

pub struct Circle(pub u32);

impl Shape for Circle {
    fn area(&self) -> u32 {
        self.0 * 3 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_circle_area() {
        let s: Box<dyn Shape> = Box::new(Circle(3));
        assert_eq!(s.area(), 10);
    }
}
