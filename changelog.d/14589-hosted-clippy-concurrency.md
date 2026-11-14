<!-- section: Internal / CI -->
- Limit hosted strict workspace Clippy to three Cargo jobs to reduce concurrent
  compiler memory pressure while retaining both feature-mode checks and all
  lint targets; scratch runners retain their configured job count (#14589).
