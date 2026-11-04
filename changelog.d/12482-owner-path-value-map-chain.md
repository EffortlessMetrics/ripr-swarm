<!-- section: Changed -->
- A bounded `.map(owner)` iterator chain whose bare path resolves to the changed owner and whose `assert_eq!` names the matching literal `Vec` now credits through the owner-return pin; unresolved paths, shadowed bindings and foreign same-name modules stay fail-closed, and the verdict-corpus row `shape-hostile-owner-as-path-value` is credited (#9478, #12482).
