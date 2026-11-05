use std::cell::RefCell;

pub struct Notifier {
    pub sent: RefCell<Vec<String>>,
    pub status: u32,
}

impl Notifier {
    pub fn new() -> Self {
        Notifier { sent: RefCell::new(Vec::new()), status: 0 }
    }
    pub fn send(&self, payload: &str) -> bool {
        let _ = self.status;
        self.sent.borrow_mut().push(payload.to_string());
        true
    }
}

pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {
    notifier.send(order_id)
}
