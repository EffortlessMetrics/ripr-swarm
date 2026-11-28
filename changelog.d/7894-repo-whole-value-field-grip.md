<!-- section: Fixed -->
- RIPR-SPEC-0225: repo field seams now reuse the bounded whole-value field pin beside legacy token matching, so adjacent expected bindings strongly grip the changed field in full and compact repo evidence (#7894).
- RIPR-SPEC-0225: a whole-value pin bound through `let c = build(..)` that the current owner does not compute (for example `retries: 5` where `build(3)` yields `4`) is now withheld from strong field grip on both the repo path and the supplied-diff path (#7894).
