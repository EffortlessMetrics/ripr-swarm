<!-- section: Internal / CI -->
- Let the reusable Rust gate accept an optional admitted exact-subject tuple and
  verify it before proof, defaulting to the unchanged legacy profile (#4987).
  The qualification profile takes a caller-supplied constituent identity and
  never reads the pull_request payload (#11988).
