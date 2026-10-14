<!-- section: Fixed -->
- Rust: a changed side effect or call inside `if <parameter> <op> <integer>`
  no longer reads `exposed` when every related test calls the owner with a
  literal that leaves the guard false (`add(&mut v, 4)` against
  `if x > 10`). The tests never run the changed line, so the finding reads
  `weakly_exposed` and names the guard as the missing input ("use an input
  that enters `if x > 10`"). A compound or named-constant guard, a computed
  input, or a test that may reach the owner through a helper keeps today's
  reading (RIPR-SPEC-0263;
  [#7132](https://github.com/EffortlessMetrics/ripr-swarm/issues/7132)).
