#[derive(Debug)]
pub struct r#Vec<T>(::std::vec::Vec<T>);
impl<T> r#Vec<T> {
    pub fn new() -> Self { Self(::std::vec::Vec::new()) }
    pub fn push(&mut self, x: T) { self.0.push(x); }
}
impl<T> PartialEq<::std::vec::Vec<T>> for r#Vec<T> {
    fn eq(&self, other: &::std::vec::Vec<T>) -> bool { self.0.len() == other.len() }
}
