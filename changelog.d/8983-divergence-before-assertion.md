<!-- section: Fixed -->
- A `loop` whose only `break` belongs to an inner loop or closure, a `while true {}`, and a `process::exit` / `abort` reached through `use std::process` (renamed or not) or an imported `exit` now end the test for the assertion-execution admission, so an assertion after them no longer credits a finding (#8983).
