#[macro_export]
macro_rules! vec { () => { impl Drop for Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } } }; }
