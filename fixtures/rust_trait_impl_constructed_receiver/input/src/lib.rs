pub trait Shape {
    fn area(&self) -> u32;
}
pub struct Square {
    side: u32,
}
pub struct Line {
    len: u32,
}
impl Square {
    pub fn with_side(side: u32) -> Square {
        Square { side }
    }
}
impl Line {
    pub fn with_len(len: u32) -> Line {
        Line { len }
    }
}
impl Shape for Square {
    fn area(&self) -> u32 {
        self.side * self.side
    }
}
impl Shape for Line {
    fn area(&self) -> u32 {
        self.len - self.len
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn square_area() {
        let square = Square::with_side(3);
        assert_eq!(square.area(), 9);
    }
    #[test]
    fn line_area() {
        let line = Line::with_len(3);
        assert_eq!(line.area(), 0);
    }
}
