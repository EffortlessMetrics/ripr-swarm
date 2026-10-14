<!-- section: Internal / CI -->
- The Rust verdict corpus covers RIPR-SPEC-0263 examples 2 to 6 with six
  runtime-labeled cases: an entering literal, one entering call among two, a
  let-bound input, a named constant bound, a recursive owner and a helper that
  calls the owner back. Rust spec-example coverage reaches 117 of 117 and the
  floor rises to 117. The let-bound and named-bound cases record the false
  `exposed` the spec's fail-closed rule accepts today (#7964).
