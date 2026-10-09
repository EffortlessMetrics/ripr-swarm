<!-- section: Fixed -->
- Module composition treats `#[r#path = "..."]` as the same `path` attribute
  as `#[path = "..."]`, so a raw-identifier redirected module gets the same
  role and parent chain as the plain spelling
  ([#7161](https://github.com/EffortlessMetrics/ripr-swarm/issues/7161)).
