<!-- section: Fixed -->
- `repair_card.v1` (`ripr agent card`, `ripr_get_repair_card`, the LSP card)
  now reports `subject.canonical_gap_id` as the same readable id
  (`gap:rust:src/lib.rs:is_adult:return_value:...`) that `ripr check`,
  `ripr_list_gaps` and `ripr_get_gap` use, so the id an agent passes back
  names the same gap on every surface. The seam-side content hash
  (`gap:<16 hex>`) the field used to carry is kept additively as
  `subject.seam_content_gap_id`. The card reports the bound finding's own id
  (null when no finding binds the seam), never an id re-minted from the seam
  (#8638).
