<!-- section: Fixed -->
- On Windows, `ripr repair` and `ripr agent status` reuse the cached full seam
  inventory for repositories above the default seam cap that fit within the
  repair limit, instead of reclassifying seams on every run (#16214).
