<!-- section: Fixed -->
- A Rust `#[test]` that `cargo test` never runs because its file is not a
  tested target no longer credits a discriminator: `src/main.rs` under
  `autobins = false`, `src/lib.rs` under `autolib = false` (and the other
  `auto*` keys), and an example under `examples/`, which `cargo test` builds
  without its tests unless `[[example]] test = true`. ripr read the changed
  owner as `exposed` (`weakly_exposed` for most example shapes); it now withholds that credit in every mode, while a
  root declared explicitly and a `test = true` example keep it
  ([#14972](https://github.com/EffortlessMetrics/ripr-swarm/issues/14972)).
