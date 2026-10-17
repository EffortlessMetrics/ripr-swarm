1. Change: `Scheduler::next` picks `min_by_key` instead of `max_by_key` on prio (src/lib.rs).
2. Ideal test: `let mut s = Scheduler::new(); s.submit("lo",1); s.submit("hi",9); assert_eq!(s.next().unwrap().name, "hi");`
3. Shape: &mut self, Option return, struct/Vec collection, prior &mut calls (submit) required.
