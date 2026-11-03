<!-- section: Fixed -->
- Pinned with a regression test that a changed function called by a module-qualified path (`crate::m1::f1(x)`, `super::m1::f1(x)`) is followed by the transitive caller walk like an imported bare call, while std-rooted and type-qualified same-named calls are not credited (#6296).
