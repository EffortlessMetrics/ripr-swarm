use std::cell::Cell;

struct Wrapper {
    inner: Cell<u32>,
}

impl Wrapper {
    fn peek(&self) -> u32 { self.get() }

    fn get(&self) -> u32 {
        { let v = self.inner.get(); v }
    }
}

#[test]
fn wrapper_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Wrapper>();
}
