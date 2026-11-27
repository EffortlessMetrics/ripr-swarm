pub struct Foo(pub u32);
impl Foo {
    pub fn new(n: u32) -> Self {
        Foo(n)
    }
}
impl Iterator for Foo {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        None
    }
}
pub trait Peeky {
    fn into_iter(&self) -> u32;
}
impl Peeky for Foo {
    fn into_iter(&self) -> u32 {
        if self.0 >= 3 { 1 } else { 0 }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn via_new() {
        assert_eq!(Foo::new(3).into_iter().count(), 0);
    }
}
