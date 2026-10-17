<!-- section: Fixed -->
- `cargo xtask check-changelog-fragments` now rejects duplicate Unreleased
  section headings in ordinary and release-cut checks, reporting both the
  first heading and each duplicate. The release-note fold keeps one
  destination per section instead of silently splitting entries (#7673).
