<!-- section: Fixed -->
- Rust: a changed match arm over a single-letter or all-caps enum variant
  (`Kind::A`) now names its missing discriminator when the enum is declared
  in another workspace file. ripr follows the owner file's one plain,
  top-level import of the type (`mod kinds; use kinds::Kind;`,
  `use crate::kinds::Kind;`) through its `mod` declarations to that file
  and reads the enum there. A renamed import, a re-export, an inline
  module or an unresolved module path still leaves the arm unknown
  ([#7675](https://github.com/EffortlessMetrics/ripr-swarm/issues/7675)).
