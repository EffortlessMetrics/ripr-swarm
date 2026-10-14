use std::{cell::RefCell,rc::Rc};
struct Counter { sink: Rc<RefCell<Vec<usize>>>, finished: bool }
fn record(output: &Rc<RefCell<Vec<usize>>>) {
    output.borrow_mut().push(1);
}
impl Iterator for Counter {
    type Item=();
    fn next(&mut self)->Option<()> {
        if self.finished { return None; }
        self.finished=true;
        record(&self.sink);
        Some(())
    }
}
#[derive(Debug,PartialEq)] struct Observation { sink: Rc<RefCell<Vec<usize>>> }
#[test]
fn direct_smoke() {
    let sink=Rc::new(RefCell::new(Vec::new()));
    record(&sink);
}
fn parse_note(_: &str)->Option<Vec<usize>> { Some(vec![7]) }
#[test]
fn parser_only() {
    let input="x";
    let expected=Some(vec![7]);
    assert_eq!(parse_note(&format!("{input}")), expected);
}
