- **Rust analysis (#9478 slice A):** a bounded `.map(owner)` iterator chain whose
  bare path resolves to the changed owner and whose `assert_eq!` names the
  matching literal `Vec` credits through the owner-return pin; unresolved paths,
  shadowed bindings and foreign same-name modules stay fail-closed. Verdict-corpus
  row `shape-hostile-owner-as-path-value` is credited; the cache schema versions
  move for the new synthetic path-value call facts.
