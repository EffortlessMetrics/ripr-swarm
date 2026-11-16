<!-- section: Internal / CI -->
- Hosted Rust gates use dev debug level zero, matching their test profile and
  the required shards. Commands and test selection stay intact; dev artifacts
  lose file/line debug metadata. The new cache environment key can make initial
  qualification cold, so whole-run savings still require measurement (#8198).
