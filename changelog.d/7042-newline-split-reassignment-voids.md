<!-- section: Fixed -->
- Same-test pairing now voids a `let`-bound boundary name when the
  reassignment is split across lines. `let mut got = gate(10); got` then
  `= true;` still overwrites the boundary result, but a per-line scan never
  saw identifier and `=` in one `;` segment, so a later `assert_eq!(got,
  true)` kept pairing. Whole-body masking plus an ordered statement scan
  voids that shape the same way a same-line `got = true` does. Unmutated
  bindings, including a multiline exact assertion, still pair (#7042 item 4).
