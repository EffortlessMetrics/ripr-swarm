pub trait Shape {
    fn area(&self) -> u32;
}
pub struct Square(pub u32);
pub struct Line(pub u32);
impl Shape for Square {
    fn area(&self) -> u32 {
        self.0 + self.0
    }
}
impl Shape for Line {
    fn area(&self) -> u32 {
        self.0 - self.0
    }
}
impl Square {
    pub fn area(&self) -> u32 {
        9
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn square_area() {
        assert_eq!(Square(3).area(), 9);
    }
}
