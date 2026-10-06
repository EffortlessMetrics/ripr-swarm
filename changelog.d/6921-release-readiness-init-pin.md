<!-- section: Changed -->
- `cargo xtask release-readiness` adds a required `init-pin-version` check. At
  the release commit (CHANGELOG.md has the version's heading), a stable release
  fails until `LATEST_RELEASED_VERSION`, the version `ripr init --ci github`
  pins, equals it; before the cut the lag only warns. A release candidate fails
  if the constant names the candidate or its stable target (#6921).
