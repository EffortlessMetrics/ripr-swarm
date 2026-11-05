use binding::Builder;
#[test]
fn observes() {
 let mut item=Builder::new();
 let _ignored={ item.attempts=4; 0 };
 assert_eq!(item.attempts,4);
}
