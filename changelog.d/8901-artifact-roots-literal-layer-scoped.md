<!-- section: Fixed -->
- Configured ripr artifact roots (`RIPR_CACHE_DIR`, `[perl].cache_dir`) are
  now scoped to what the writers actually own and probed literally:
  a relocated seam-cache base contributes only its writer layer
  directories, so pointing the cache at a directory that also holds source
  (e.g. `RIPR_CACHE_DIR=src`) no longer erases real files from analysis;
  doctor's git probes escape pathspec glob magic, so a root like
  `build/cache*` no longer wildcard-matches `build/cache1`; the printed
  untrack command names only indexed roots and shell-quotes every
  pathspec; and the diff artifact disclosure recommends the `.gitignore`
  entries for the roots that actually excluded the files instead of a
  stale `/target/ripr/` rule (#8751; PR #8901 review).
