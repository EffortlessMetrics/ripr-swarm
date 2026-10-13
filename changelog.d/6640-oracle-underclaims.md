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
  own result. Verdict-corpus oracle matches rise from 26/34 to 29/34 with no
  underclaims left, and the `match_floor` gate is raised to 29
  ([#6640](https://github.com/EffortlessMetrics/ripr-swarm/issues/6640)).
