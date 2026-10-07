<!-- section: Docs -->
- `cargo xtask check-changelog-fragments`, run by `precommit`, rejects a
  `changelog.d/` fragment with no or an unknown section line, an entry that is
  not a `- ` bullet, no issue or PR reference, or a name that is not a
  lowercase kebab slug. `--release-cut` also fails on any unfolded fragment
  (#6341).
