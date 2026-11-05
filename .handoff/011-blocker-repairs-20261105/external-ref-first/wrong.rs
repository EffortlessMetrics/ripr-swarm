use std::cell::RefCell;

pub struct Notifier {
    pub sent: RefCell<Vec<String>>,
}

impl Notifier {
    pub fn new() -> Self {
        Notifier { sent: RefCell::new(Vec::new()) }
    }
    pub fn send(&self, payload: &str) -> bool {
        self.sent.borrow_mut().push(payload.to_string());
        true
    }
}

pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {
    notifier.send("static_msg");
    true
}

#[cfg(test)]
mod tests {
use super::*;
use external_first::First;

#[test]
fn process_order_sends_exact_order_id() {
let notifier = Notifier::new();
process_order(&notifier, "order-42");
assert!(matches!(notifier.sent.borrow().first().map(String::as_str), Some("order-42")));
}
}
