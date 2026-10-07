<!-- section: Fixed -->
- `ripr pilot` ranks the current change on repos past the inventory seam limit:
  when `RIPR_REPO_EXPOSURE_SEAM_LIMIT` cuts the inventory, pilot classifies the
  change's Rust files on their own and adds the seams on changed lines the cut
  dropped, so they rank change-first instead of being invisible (#6943).
