pub trait Make {
    fn make(n: u32) -> Self;
}
pub struct Square(pub u32);
impl Make for Square {
    fn make(n: u32) -> Self {
        Square(n * 3)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn makes() {
        assert_eq!(<Square as Make>::make(2).0, 4);
    }
}
