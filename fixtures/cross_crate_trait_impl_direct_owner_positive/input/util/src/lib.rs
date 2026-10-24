pub mod io;
pub mod stream;

pub fn block_on<T>(f: impl FnOnce() -> T) -> T {
    f()
}
