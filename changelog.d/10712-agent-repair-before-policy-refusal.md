<!-- section: Fixed -->
- `ripr agent repair --phase before` now exits 3 (typed refusal) for a seam
  the inventory has but agent policy withholds, matching `agent card`,
  `agent start`, and `agent packet`. An unknown `--seam-id` takes the same
  typed exit-3 path. `agent stub` on a policy-omitted seam still produces the
  stub (exit 0, suggestion text rather than a repair start) with the omission
  disclosed in `warnings` and no `next_repair` journey offered (#10712).
