<!-- section: Changed -->
- `cargo xtask release-readiness` adds a required `init-pin-version` check on
  `LATEST_RELEASED_VERSION`, the version `ripr init --ci github` pins. At the
  release commit (CHANGELOG.md has the version's heading) it fails until the
  constant equals the release version. Before the cut a lagging constant only
  warns, and one already at or past the version fails. A release candidate
  fails unless the constant names a release older than its stable target
  (#6921).
