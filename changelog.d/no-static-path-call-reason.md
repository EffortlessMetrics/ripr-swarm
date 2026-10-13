<!-- section: Fixed -->
- Rust: in a `no_static_path` finding, a related test linked only by file,
  module, name token, or seam callee now gives `no_call_path` as its reason.
  ripr used to report an assertion-level reason first ("assertion not
  credited"), which pointed at assertion admission instead of the missing
  call. The verdict corpus now counts `no_static_path_with_related_tests` only
  when a listed test's relation reaches the owner, which matches #5424's
  listing of every examined test, so the bytesize `as_kb`/`as_mib` rows no
  longer count as contradictions
  ([#6580](https://github.com/EffortlessMetrics/ripr-swarm/issues/6580)).
