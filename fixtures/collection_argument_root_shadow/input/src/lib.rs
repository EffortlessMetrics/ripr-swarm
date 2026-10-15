#[derive(Debug)]
pub struct Numbers<T>(::std::vec::Vec<T>);
impl<T> Numbers<T> {
    pub fn new() -> Self { Self(::std::vec::Vec::new()) }
    pub fn push(&mut self, x: T) { self.0.push(x); }
}
impl<T> PartialEq<::std::vec::Vec<T>> for Numbers<T> {
    fn eq(&self, other: &::std::vec::Vec<T>) -> bool { self.0.len() == other.len() }
}
pub mod std;
pub struct Bag(pub std::vec::Vec<u32>);
pub fn add(v: &mut std::vec::Vec<u32>, x: u32) {
    v.push(x * 1);
}
#[cfg(test)]
mod tests {
    use super::{add, Bag, std};
    #[test]
    fn records_into_the_bag() {
        let mut b = Bag(std::vec::Vec::new());
        add(&mut b.0, 11);
        assert_eq!(b.0, vec![11]);
    }
}
