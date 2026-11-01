<!-- section: Fixed -->
- An exact assertion inside the closure of `loom::model` or `shuttle::check_*` no longer reads `reachable_unrevealed` with "make the check run on every default test run"; ripr does not read the runner, so the finding is `static_unknown` with a stated analyzer limit (#11543).
- The "owner is itself an assertion helper" advice no longer appears for ordinary logic written as a one-line `fn`, or for a function whose body only unwraps a value with `.expect(` (#11546).
