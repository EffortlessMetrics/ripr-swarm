pub trait Counter {
    fn step(&self) -> u32;
    fn advance(&self) -> u32 {
        4 + self.step()
    }
}
pub struct Unit;
impl Counter for Unit {
    fn step(&self) -> u32 {
        2
    }
    fn advance(&self) -> u32 {
        8
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advances() {
        assert_eq!(Unit.advance(), 8);
    }
}
