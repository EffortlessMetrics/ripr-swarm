pub trait Shape {
    const SIDES: u32;
    fn area(&self) -> f64;
    /// Default method: classify by area.
    fn describe(&self) -> String {
        if self.area() > 100.0 { "large".into() } else { "small".into() }
    }
}

pub struct Square(pub f64);
pub struct Tri(f64, f64);
impl Tri {
    pub fn new(b: f64, h: f64) -> Self { Tri(b, h) }
}

impl Shape for Square {
    const SIDES: u32 = 4;
    fn area(&self) -> f64 { self.0 * self.0 }
}
impl Shape for Tri {
    const SIDES: u32 = 3;
    fn area(&self) -> f64 { self.0 * self.1 / 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_and_large() {
        assert_eq!(Square(2.0).describe(), "small");
        assert_eq!(Square(20.0).describe(), "large");
    }
    #[test]
    fn tri() {
        assert_eq!(Tri::new(4.0, 3.0).area(), 6.0);
        assert_eq!(<Tri as Shape>::SIDES, 3);
    }
}
