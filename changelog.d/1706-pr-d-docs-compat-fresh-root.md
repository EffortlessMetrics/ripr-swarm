<!-- section: Changed -->
- Retired the deprecated singleton `active goal` / `current work item`
  authority wording from live guidance now that the selected-work consumers
  pass: the source-of-truth stack, CI fallback boundary, doc library, Codex
  goals pointer, closeout and handoff templates, roadmap operating-truth
  chain, and repo tracking translation table now name the portfolio-selected
  work vocabulary or mark the retired manifest historical; RIPR-SPEC-0060
  and RIPR-PROP-0015 carry explicit amendments recording that the
  active-goal validator and `cargo xtask check-goals` left with the
  `.ripr/goals/` scheduler in #1701 (#1706).
<!-- section: Added -->
- RIPR-SPEC-0244 records the legacy compatibility migration posture: the
  `legacy_active_goal_ref` / `legacy_current_work_item_ref` fields stay
  read-only, resolve one way into `historical` campaign references, grant no
  write/readiness/merge/closeout authority in any of the nine PR B consumer
  checks, and retire by omission rather than automation (#1706).
- A committed fresh-root reconstruction receipt
  (`fixtures/work_selection_identity/fresh_root_reconstruction.json`) binds
  the shared selection corpus plus the nine PR B consumer corpora by
  corpus/provenance SHA-256 digests, scenario/case counts, and pass state,
  so a fresh process rebuilds the whole selected-work contract state from
  committed artifacts alone — no chat history, no singleton goal file, no
  live GitHub state. `cargo xtask check-fixture-contracts` validates the
  receipt's structure and digest bindings, and the
  `work_selection_identity_fresh_root_receipt` tests re-run all ten domain
  checks in a fresh process against the receipt, including negative
  controls for digest drift and a dropped domain (#1706).
