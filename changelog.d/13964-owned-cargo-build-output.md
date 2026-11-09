<!-- section: Internal / CI -->
- Cargo preserves the build owner's prepared temporary directories, developer
  setup and doctor follow effective temp selection, and the reusable Rust gate
  explicitly disables disposable incremental output. Full development validation
  checks both effective Cargo output directories before compilation. Agent guidance
  reuses one private target and separates its cleanup from retained reports (#13964).
