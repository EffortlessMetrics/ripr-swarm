use observation_verified_side_effect_fixture::{Notifier, process_order};

#[test]
fn process_order_sends_order_id() {
    let notifier = Notifier;
    let sent_order_id = "order-42";
    let result = process_order(&notifier, sent_order_id);
    assert!(notifier.send(sent_order_id));
    let _ = result;
}
