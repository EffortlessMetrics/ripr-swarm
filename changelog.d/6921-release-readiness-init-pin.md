<!-- section: Changed -->
- `cargo xtask release-readiness` adds a required `init-pin-version` check: a
  stable release fails until `LATEST_RELEASED_VERSION` (the version
  `ripr init --ci github` pins) equals the release version, and a release
  candidate fails if the constant names it (#6921).
