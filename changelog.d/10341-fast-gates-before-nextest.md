<!-- section: Fixed -->
- The required Rust run now executes the golden, fixture, capability and verdict-corpus gates before the 18-minute nextest step, so a main-side drift fails the run within minutes instead of after the tests (#10341).
