pub struct Notifier { pub sent:Vec<String>, pub status:u32 }
impl Notifier {
 pub fn new()->Self { Self { sent:vec![], status:0 } }
 pub fn send(&mut self,payload:&str)->bool {
  self.status=1;
  self.sent.push(payload.to_string());
  true
 }
}
pub fn process_order(notifier:&mut Notifier,order_id:&str)->bool {
 notifier.send(order_id)
}
