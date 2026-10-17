1. Change: transition `(Running, Fail) => State::Failed` -> `State::Done` (src/lib.rs).
2. Ideal test: `let mut m = Machine::new(); m.step(Event::Start).unwrap(); assert_eq!(m.step(Event::Fail), Ok(State::Failed)); assert_eq!(m.state(), State::Failed);`
3. Shape: &mut self, returns Result, enum variant transition, prior &mut call (Start) required.
