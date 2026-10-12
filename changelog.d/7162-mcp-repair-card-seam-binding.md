<!-- section: Fixed -->
- `ripr_get_repair_card` / `ripr://repair-card/{canonical_id}` no longer fail
  with `seam_not_found` on real snapshots: the seam-to-item join now runs
  through the shared seam-finding authority (the finding's producer-structured
  file, owner, and probe family) instead of demanding string equality between
  the seam's content-hash gap id and the finding's producer gap id, two
  schemes that never coincide. Owner discrimination, the ambiguity refusal,
  and the `seam_not_found` spelling are unchanged; the CLI, LSP, and MCP card
  projections share the one join, so one item still yields one card on every
  transport (#7162).
