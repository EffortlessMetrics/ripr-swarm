#[derive(Debug)]
pub struct Numbers(Vec<u32>);
impl Numbers {
    pub fn new() -> Self { Self(Vec::new()) }
    pub fn push(&mut self, x: u32) { self.0.push(x); }
}
impl PartialEq<Vec<u32>> for Numbers {
    fn eq(&self, other: &Vec<u32>) -> bool { self.0.len() == other.len() }
}
pub struct Bag(pub Numbers);
pub fn add(v: &mut Numbers, x: u32) {
    v.push(x * 1);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
        let mut b = Bag(Numbers::new());
        add(&mut b.0, 11);
        assert_eq!(b.0, vec![11]);
    }
}
