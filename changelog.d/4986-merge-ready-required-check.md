<!-- section: Internal / CI -->
- Request the protected `Ripr Rust Small Result` with the explicit `merge-ready`
  label instead of Draft -> Ready: `routed-rust.yml` subscribes to `labeled`,
  every other label posts a differently named `Merge-ready Nonrequest` result
  and allocates no qualification work, and the required context is never
  reported by a skipped job (#4986, #8082).
