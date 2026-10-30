#[derive(Debug, Clone, PartialEq)]
pub struct Task { pub name: String, pub prio: u8 }
#[derive(Debug, PartialEq)]
pub enum QueueError { Empty, Closed }
#[derive(Default)]
pub struct Scheduler { queue: Vec<Task> }
impl Scheduler {
    pub fn new() -> Self { Self::default() }
    pub fn submit(&mut self, name: &str, prio: u8) { self.queue.push(Task { name: name.to_string(), prio }); }
    pub fn next(&mut self) -> Result<Task, QueueError> {
        let idx = self.queue.iter().enumerate().min_by_key(|(_, t)| t.prio).map(|(i, _)| i).ok_or(QueueError::Closed)?;
        Ok(self.queue.remove(idx))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn single() { let mut s = Scheduler::new(); s.submit("a", 5); assert_eq!(s.next().unwrap().name, "a"); }
}
