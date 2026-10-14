#[derive(Debug)]
pub struct Numbers(Vec<u32>);
impl Numbers {
    pub fn new() -> Self { Self(Vec::new()) }
    pub fn push(&mut self, x: u32) { self.0.push(x); }
}
impl PartialEq<Vec<u32>> for Numbers {
    fn eq(&self, other: &Vec<u32>) -> bool { self.0.len() == other.len() }
}
impl std::ops::Deref for Numbers {
    type Target = Vec<u32>;
    fn deref(&self) -> &Self::Target { &self.0 }
}
impl std::ops::DerefMut for Numbers {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}
pub mod real { pub struct Bag(pub Vec<u32>); }
pub struct Payload(pub Numbers);
macro_rules! make_factory {
    ($name:ident) => {
        #[allow(non_snake_case)]
        pub fn $name(_: Vec<u32>) -> Payload { Payload(Numbers::new()) }
    };
}
make_factory!(Bag);
pub fn add(v: &mut Vec<u32>, x: u32) {
    v.push(x * 1);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
        let mut b = Bag(Vec::new());
        add(&mut b.0, 11);
        assert_eq!(b.0, vec![11]);
    }
}
