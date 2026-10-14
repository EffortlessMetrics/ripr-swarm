<!-- section: Fixed -->
- Rust: an input that a narrowed or widened match-arm pattern moved onto a
  later arm over a single-letter enum variant (`Kind::B => 2`) now credits
  the changed arm. Later arms read declared uncased variants the same way
  the changed and earlier arms do
  ([#7670](https://github.com/EffortlessMetrics/ripr-swarm/issues/7670)).
