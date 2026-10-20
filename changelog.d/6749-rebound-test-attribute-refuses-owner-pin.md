<!-- section: Fixed -->
- A bare `#[test]` no longer earns an owner-return pin when its module imports another attribute macro under the name `test` (`use skipper::serial as test;`), directly, through `use super::*`, through `extern crate .. as test`, or in a file that generates the test with a macro; such a test may run no body at all, so the verdict was a false `exposed` (#6749).
