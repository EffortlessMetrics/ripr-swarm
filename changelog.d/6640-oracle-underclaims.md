<!-- section: Fixed -->
- Rust oracles: three test shapes that ripr related to a changed error but
  read as having no oracle now get the reading RIPR-SPEC-0227 and
  RIPR-SPEC-0231 adopt (for inequalities, the sibling-exclusion case only).
  An `unwrap_err()` whose value is never compared reads `broad_error` /
  weak, so its finding is `weakly_exposed` instead of
  `reachable_unrevealed`. `assert_ne!(check(20), Err(E::Bad))` and
  `assert!(!matches!(check(20), Err(E::Bad)))` against a changed
  `Err(E::Big)` read `relational_check` / weak instead of being dropped as
  sibling-variant pins, because they exclude the sibling from the owner's
  own result. This removes the last 3 verdict-corpus oracle underclaims;
  with RIPR-SPEC-0231 rule 1 (#6670) the corpus oracle rows read 34/34, so
  the `match_floor` gate is raised to 34 and `overclaim_ceiling` lowered to
  0 ([#6640](https://github.com/EffortlessMetrics/ripr-swarm/issues/6640)).
