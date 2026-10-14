<!-- section: Fixed -->
- Rust: a `no_static_path` finding that ripr withholds as `static_unknown`,
  because tests call a helper whose name is not unique, again reports each
  related test's assertion-level miss reason instead of none
  ([#7532](https://github.com/EffortlessMetrics/ripr-swarm/issues/7532)).
- Rust: reach, the `no_static_path` miss reason, and the verdict corpus
  contradiction check now read one rule for which test relations reach the
  owner. After #5335 stopped counting name, import and fixture affinity as
  reach, the miss reason and the corpus still did: a `no_static_path`
  finding showed an assertion-level reason for a test named after the owner,
  and the corpus counted that test as a contradiction. Both now give
  `no_call_path` and no contradiction, and the corpus treats an unrecognized
  relation as reaching
  ([#7531](https://github.com/EffortlessMetrics/ripr-swarm/issues/7531)).
