<!-- section: Internal / CI -->
- `.github/workflows/routed-rust.yml` raises the per-candidate
  `max-changed-lines` admission cap from 3000 to 3700 in both the queue and
  qualification jobs, so a candidate whose repair is real but larger than the
  old cap can qualify (#14513).
