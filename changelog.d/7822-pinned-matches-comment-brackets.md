<!-- section: Fixed -->
- A pinned `matches!` compared with a bool no longer loses exact credit
  when a nested or line comment holds unmatched brackets:
  `ensure!(matches!(r, /* } */ Ok(3)) == true)` now reads `exact_value` /
  strong. An unpinned pattern with the same comment stays
  `relational_check` / weak (RIPR-SPEC-0231 rule 3; #7822).
