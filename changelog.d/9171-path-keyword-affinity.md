<!-- section: Fixed -->
- A Rust path or declaration keyword (`crate`, `super`, `const`, `static`,
  `match`, ...) in an owner-less probe no longer relates tests through
  assertion-target affinity. A changed `pub(crate) use ..` line used to report
  `reach yes` and credit the exact-value oracle of any test that asserts on a
  `crate::..` path. The declaration introducers `union Name {..}` and
  `macro_rules!` (legal identifiers elsewhere) are refused the same way on both
  the probe and the assertion side, while a genuine identifier use such as
  `a.union(&b)` still relates (#9171).
