use std::{cell::Cell, rc::Rc};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl Iterator for Counter {
    type Item = ();
    fn next(&mut self) -> Option<()> {
        if self.finished { return None; }
        self.finished = true;
        self.sink.set(1);
        Some(())
    }
}
#[derive(Debug, PartialEq)] struct Observation { cell: Rc<Cell<usize>> }
#[test]
fn direct_smoke() {
    let mut counter = Counter { sink: Rc::new(Cell::new(0)), finished: false };
    let _ = counter.next();
}
#[test]
fn implicit_observer() {
    let state = Rc::new(Cell::new(0));
    let counter = Counter { sink: state.clone(), finished: false };
    for _ in counter {}
    assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });
}
