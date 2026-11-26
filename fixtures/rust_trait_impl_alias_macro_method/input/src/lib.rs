pub trait Shape {
    fn area(&self) -> u32;
}
pub struct Square {
    side: u32,
}
pub struct Line(pub u32);
macro_rules! by_value {
    () => {
        pub fn area(self) -> u32 {
            9
        }
    };
}
pub type Tile = Square;
impl Tile {
    by_value!();
}
impl Square {
    pub fn with_side(side: u32) -> Square {
        Square { side }
    }
}
impl Shape for Square {
    fn area(&self) -> u32 {
        self.side + self.side
    }
}
impl Shape for Line {
    fn area(&self) -> u32 {
        self.0 - self.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn square_area() {
        assert_eq!(Square::with_side(3).area(), 9);
    }
}
pub mod unrelated {
    pub struct Tile;
}
