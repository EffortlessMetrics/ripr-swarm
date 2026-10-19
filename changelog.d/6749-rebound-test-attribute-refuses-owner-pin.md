<!-- section: Fixed -->
- A bare `#[test]` no longer earns an owner-return pin when its module imports another attribute macro under the name `test` (`use skipper::serial as test;`), directly, through `use super::*`, or through a glob of a crate outside the workspace; such a test may run no body at all, so the verdict was a false `exposed` (#6749).
