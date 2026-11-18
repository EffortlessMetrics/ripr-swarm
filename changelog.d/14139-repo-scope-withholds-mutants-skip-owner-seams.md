<!-- section: Fixed -->
- The repo-scope formats of `ripr check` (`repo-exposure-json`,
  `repo-exposure-summary-json`, `repo-exposure-md`, `repo-seams-json`,
  `repo-seams-md`, `repo-badge-json`, `repo-badge-shields`,
  `repo-badge-plus-json`, `repo-badge-plus-shields`) no longer count the
  seams of an owner function that carries `#[mutants::skip]` (directly or
  through `cfg_attr`) as exposure gaps. They are withheld from the seam lists
  and every count, and the JSON formats disclose the number in a top-level
  `owner_opt_outs` member (`{"withheld": N, "reason": "owner_mutants_skip",
  "seam_ids": [...], "owners": [...]}`) emitted only when something was
  withheld. `ripr outcome` and `ripr agent verify` exclude a seam withheld in
  either snapshot, by id or by owner, from both sides and disclose it, so a
  withheld seam is never reported as resolved, even when inserting the
  attribute line moved its ids (#14139).
