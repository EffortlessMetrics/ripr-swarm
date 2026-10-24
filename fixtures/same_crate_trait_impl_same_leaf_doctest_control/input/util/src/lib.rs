pub mod io;
pub mod stream;

pub trait Stream {
    fn size_hint(&self, upper: Option<usize>) -> Option<usize>;
}

pub fn block_on<T>(f: impl FnOnce() -> T) -> T {
    f()
}
