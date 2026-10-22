<!-- section: Internal / CI -->
- Pinned a reproducible measurement harness for the repeated inherent-receiver
  parsing cost named in #8005: a representative many-seams fixture whose owners
  share the same small tests, behavioral witnesses that keep
  `direct_owner_call` attribution, byte-stable related-test inventories across
  repeated runs, and full/compact repo-exposure grip-count agreement, plus an
  on-demand wall-time sampling probe at three scales. The parser-invocation
  accounting (once per distinct source text) is verified in the #7415 lane
  where the source-snapshot memo infra it counts lands; the harness and
  witnesses here keep the cost question reproducible on main (#8005).
