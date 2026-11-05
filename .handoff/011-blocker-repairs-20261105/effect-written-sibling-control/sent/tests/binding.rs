use binding::{Notifier,process_order};
#[test]
fn observes() {
 let mut notifier=Notifier::new();
 process_order(&mut notifier,"order-42");
 assert_eq!(notifier.sent,vec!["order-42".to_string()]);
}
