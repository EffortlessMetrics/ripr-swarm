pub trait Stream {
    fn size_hint(&self, upper: Option<usize>) -> Option<usize>;
}
