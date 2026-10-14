use std::ops::Add;
use std::{cell::Cell, rc::Rc};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl std::ops::Add<()> for Counter {
    type Output = ();
    fn add(self, _: ()) {
        self.sink.set(0);
    }
}
#[derive(Debug, PartialEq)] struct Observation { cell: Rc<Cell<usize>> }
#[test]
fn direct_smoke() {
    let counter = Counter { sink: Rc::new(Cell::new(0)), finished: false };
    let _ = counter.add(());
}
#[test]
fn implicit_observer() {
    let sink = Rc::new(Cell::new(0));
    let counter = Counter { sink: sink.clone(), finished: false };
    let _ = counter + ();
    assert_eq!(sink.get(), 1);
}
