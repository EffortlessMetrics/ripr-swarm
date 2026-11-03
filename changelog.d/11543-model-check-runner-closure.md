<!-- section: Fixed -->
- An exact assertion inside the closure of `loom::model` or `shuttle::check_*` no longer reads `reachable_unrevealed` with "make the check run on every default test run"; ripr does not read the runner, so the finding is `static_unknown` with a stated analyzer limit (#11543).
