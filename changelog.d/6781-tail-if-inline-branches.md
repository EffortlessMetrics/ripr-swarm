<!-- section: Fixed -->
- A predicate written as a single-line tail `if`/`else` (for example
  `if quantity >= 7 { 20 } else { 0 }`) now flows to the owner's return value
  during static exposure analysis, instead of reading `propagation_unknown`
  when a test already pins the boundary result. The multi-line form of the
  same code already classified correctly; both forms now agree (#6781).
