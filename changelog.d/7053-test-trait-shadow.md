<!-- section: Fixed -->
- A test that calls a method through a trait the test module redeclares under the same name no longer reads as a direct call to the production `impl Trait for T`; the relation stays name-only ([#7053](https://github.com/EffortlessMetrics/ripr-swarm/issues/7053)).
