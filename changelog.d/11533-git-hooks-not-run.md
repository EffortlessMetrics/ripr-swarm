<!-- section: Fixed -->
- Analysis no longer runs a repository's `post-index-change` hook (from
  `core.hooksPath` or `.git/hooks`): ripr's git calls now take no optional
  index lock, so `git status` cannot rewrite a stale index, and pin
  `core.hooksPath` to the null device (#11533).
