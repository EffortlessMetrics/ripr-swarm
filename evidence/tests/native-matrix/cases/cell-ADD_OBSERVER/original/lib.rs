use std::{cell::Cell, rc::Rc, ops::Add};
struct Counter { sink: Rc<Cell<usize>>, finished: bool }
impl std::ops::Add<()> for Counter {
    type Output = ();
    fn add(self, _: ()) {
        self.sink.set(1);
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
    let state = Rc::new(Cell::new(0));
    let counter = Counter { sink: state.clone(), finished: false };
    let _ = counter + ();
    assert_eq!(Observation { cell: state }, Observation { cell: Rc::new(Cell::new(1)) });
}
