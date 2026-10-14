<!-- section: Internal / CI -->
- The Rust verdict corpus covers RIPR-SPEC-0263 examples 2 to 6 with nine
  runtime-labeled cases: an entering literal, one entering call among two, a
  let-bound input, a named constant bound, and five example-6 re-entries (a
  recursive owner, a helper and a local macro that call the owner back, a
  renamed import and an `AddAssign` impl). Rust spec-example coverage reaches
  117 of 117 and the floor rises to 117. The let-bound and named-bound cases
  record the false `exposed` the spec's fail-closed rule accepts today (#7964),
  and an uncited case records a push into a field-projected argument that reads
  `weakly_exposed` although the test pins it (#7966), so Rust corpus false
  verdicts move from 43 of 462 to 46 of 472.
