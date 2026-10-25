<!-- section: Changed -->
- An exact pin of a `std::thread::spawn(move || e).join().unwrap()` owner tail is credited: the closure runs once on every call, so the change is read inside `e` (RIPR-SPEC-0265 rule 4). A branch or short circuit in `e`, an unqualified `thread::spawn` and `unwrap_or` keep the earlier reading (#9490).
