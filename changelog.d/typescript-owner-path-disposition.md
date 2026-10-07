<!-- section: Changed -->
- Each TypeScript related-test candidate now carries an internal owner-path
  disposition naming what its relation and identity gates established about
  the changed owner: a trusted owner path, a module-entry path, an unanchored
  owner-name call, heuristic-only evidence, an unresolved alias, or an
  affirmative rejection (local shadow, unrelated import or destructure,
  owner-module mock, fabricated spy). `candidate_observes_owner_call` is now
  a projection of that disposition rather than a second run of the gates.
  Findings, relations, confidence, ranking and rendered output are unchanged
  (#5523).
