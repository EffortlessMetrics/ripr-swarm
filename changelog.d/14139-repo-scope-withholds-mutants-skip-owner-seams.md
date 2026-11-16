<!-- section: Fixed -->
- The repo-scope formats of `ripr check` (`repo-exposure-json`,
  `repo-exposure-summary-json`, `repo-exposure-md`, `repo-seams-json`,
  `repo-seams-md`, `repo-badge-json`, `repo-badge-shields`) no longer count the
  seams of an owner function that carries `#[mutants::skip]` (directly or
  through `cfg_attr`) as exposure gaps. They are withheld from the seam lists
  and every count, and the JSON formats disclose the number in a top-level
  `owner_opt_outs` member (`{"withheld": N, "reason": "owner_mutants_skip"}`)
  emitted only when something was withheld (#14139).
