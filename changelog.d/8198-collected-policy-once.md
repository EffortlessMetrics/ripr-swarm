<!-- section: Internal / CI -->
- Required Rust CI runs dependency, process and network policies once through
  `ci-policy`, retaining their failure propagation and leaf reports without
  repeating the three source scans or reporting nonexistent step outcomes (#8198).
