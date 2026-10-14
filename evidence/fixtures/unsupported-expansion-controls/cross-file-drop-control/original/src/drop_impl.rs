impl Drop for super::Counter { fn drop(&mut self) { self.sink.borrow_mut().clear(); self.sink.borrow_mut().push(1); } }
