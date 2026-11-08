<!-- section: Changed -->
- A finding retargeted from a changed `let` onto its predicate uses now names
  the changed `let` declaration in its `binding_predicate_relation` evidence
  (`changed \`let x\` declared at line N`), so the reader can connect the
  unchanged predicate line to the changed binding and initializer; seeding is
  unchanged (#12118).
