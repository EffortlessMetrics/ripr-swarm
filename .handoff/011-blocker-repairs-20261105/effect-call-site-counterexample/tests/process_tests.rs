use observation_verified_side_effect_fixture::{Notifier, process_order};
#[test]
fn ordinary() {
 let notifier = Notifier::new();
 let changed = Notifier::new();
 process_order(&changed, "order-42");
 assert_eq!(*notifier.sent.borrow(), Vec::<String>::new());
}
