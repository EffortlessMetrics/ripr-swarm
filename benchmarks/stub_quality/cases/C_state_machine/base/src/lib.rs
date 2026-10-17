#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State { Idle, Running, Done, Failed }
#[derive(Debug, Clone, Copy)]
pub enum Event { Start, Finish, Fail, Reset }
pub struct Machine { state: State, steps: u32 }
impl Machine {
    pub fn new() -> Self { Machine { state: State::Idle, steps: 0 } }
    pub fn state(&self) -> State { self.state }
    pub fn step(&mut self, ev: Event) -> Result<State, String> {
        let next = match (self.state, ev) {
            (State::Idle, Event::Start) => State::Running,
            (State::Running, Event::Finish) => State::Done,
            (State::Running, Event::Fail) => State::Failed,
            (_, Event::Reset) => State::Idle,
            (s, e) => return Err(format!("invalid {:?} in {:?}", e, s)),
        };
        self.state = next;
        self.steps += 1;
        Ok(next)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn happy_path() { let mut m = Machine::new(); m.step(Event::Start).unwrap(); assert_eq!(m.step(Event::Finish), Ok(State::Done)); }
    #[test] fn invalid() { let mut m = Machine::new(); assert!(m.step(Event::Finish).is_err()); }
    #[test] fn reset() { let mut m = Machine::new(); m.step(Event::Start).unwrap(); m.step(Event::Reset).unwrap(); assert_eq!(m.state(), State::Idle); }
    #[test] fn fail_is_ok() { let mut m = Machine::new(); m.step(Event::Start).unwrap(); assert!(m.step(Event::Fail).is_ok()); }
}
