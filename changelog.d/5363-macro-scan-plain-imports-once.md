<!-- section: Changed -->
- The trusted-macro binding scan decides whether a `use std::..` root is
  rebound by a plain import from one name set per file instead of walking the
  whole syntax tree for every such `use`. Verdicts are unchanged; a warm
  `ripr check` on this repository measured about 11% faster in one
  back-to-back comparison (#5363, #7220).
