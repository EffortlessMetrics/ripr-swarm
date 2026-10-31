<!-- section: Internal / CI -->
- Added `cargo xtask fold-changelog [--dry-run | --write] [--limit N]`, a repo-owned fold of `changelog.d/` fragments into `CHANGELOG.md` `Unreleased` (dry run by default, deterministic, refuses malformed fragments by file name), so the release cut no longer depends on an external fold kit (#5114).
