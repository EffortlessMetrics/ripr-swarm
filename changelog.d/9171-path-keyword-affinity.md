<!-- section: Fixed -->
- The Rust declaration introducers `union Name {..}` and `macro_rules!` (legal
  identifiers elsewhere) no longer seed assertion-target affinity: they are
  refused the same way on both the probe and the assertion side, while a
  genuine identifier use such as `a.union(&b)` still relates. (The earlier
  `pub(crate) use ..` reach-yes false relation, caused by reserved keywords such
  as `crate` seeding affinity, was fixed by #9172 and #9205.) A changed
  `pub union Name {` or `macro_rules!` line is now covered by the
  `rust_path_keyword_affinity` fixture (#9171).
