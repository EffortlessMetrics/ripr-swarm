<!-- section: Changed -->
- A repo-wide `ripr check --format repo-exposure-json` cold run computes the
  set of functions that can reach a changed owner once per owner name instead
  of once per seam. On a 4-core host a full rust-lang/cargo scan (28,977
  seams) went from 81 s to 47 s and rust-lang/rust-analyzer (83,163 seams)
  from 236 s to 150 s; the documents are unchanged. Peak memory on those
  scans rose by about 300 MB, because the sets are kept for the run (#16022).
