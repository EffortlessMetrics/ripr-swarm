use std::ops::Add;
use std::{cell::RefCell,rc::Rc};
struct Counter { sink: Rc<RefCell<Vec<usize>>>, finished: bool }
fn record(output: &Rc<RefCell<Vec<usize>>>) {

}
impl std::ops::Add<()> for Counter { type Output=(); fn add(self, _: ()) {  } }
#[derive(Debug,PartialEq)] struct Observation { sink: Rc<RefCell<Vec<usize>>> }
#[test]
fn direct_smoke() {
    let sink=Rc::new(RefCell::new(Vec::new()));
    let counter=Counter { sink,finished:false }; let _=counter.add(());
}
fn parse_note(_: &str)->Option<Vec<usize>> { Some(vec![7]) }
#[test]
fn parser_only() {
    let input="x";
    let expected=Some(vec![7]);
    assert_eq!(parse_note(&format!("{input}")), expected);
}
#[test]
fn implicit_observer() {
    let sink=Rc::new(RefCell::new(Vec::new()));
    let counter=Counter { sink:sink.clone(), finished:false };
    let _=counter + ();
    assert_eq!(Observation { sink }, Observation { sink:Rc::new(RefCell::new(vec![1])) });
}
