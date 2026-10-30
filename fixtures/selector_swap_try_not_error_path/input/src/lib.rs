#[derive(Debug, Clone, PartialEq)]
pub struct Task { pub name: String, pub prio: u8 }
#[derive(Default)]
pub struct Scheduler { queue: Vec<Task> }
impl Scheduler {
    pub fn new() -> Self { Self::default() }
    pub fn submit(&mut self, name: &str, prio: u8) { self.queue.push(Task { name: name.to_string(), prio }); }
    pub fn next(&mut self) -> Option<Task> {
        let idx = self.queue.iter().enumerate().min_by_key(|(_, t)| t.prio).map(|(i, _)| i)?;
        Some(self.queue.remove(idx))
    }
    pub fn pending(&self) -> usize { self.queue.len() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn empty() { assert_eq!(Scheduler::new().next(), None); }
    #[test] fn single() { let mut s = Scheduler::new(); s.submit("a", 5); assert_eq!(s.next().unwrap().name, "a"); assert_eq!(s.pending(), 0); }
    #[test] fn drains_all() { let mut s = Scheduler::new(); s.submit("a", 1); s.submit("b", 2); s.next(); s.next(); assert!(s.next().is_none()); }
}
