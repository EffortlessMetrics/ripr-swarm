<!-- section: Changed -->
- Warm `ripr check --format repo-exposure-json` runs are faster. The per-seam
  JSON renderer now writes fields straight into the output buffer instead of
  `format!` temporaries, and string escaping reuses the buffer. Median warm
  rerun on the 10k-seam perf target drops from 2.53 s to 2.05 s (-19%), with
  the seam prerender span down 35%. Output bytes are unchanged (#6898).
