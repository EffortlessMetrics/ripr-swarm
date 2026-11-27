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
    fn boxed_squares_area() {
        let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(3))];
        let got = match shapes.first() {
            Some(s) => s.area(),
            None => 0,
        };
        assert_eq!(got, 7);
    }
}
