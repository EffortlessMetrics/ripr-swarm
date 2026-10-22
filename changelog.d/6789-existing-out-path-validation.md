<!-- section: Fixed -->
- `ripr pr-comments existing --out` now refuses absolute paths, `..`,
  symlink components, and tab/CR/LF, matching `requests --out-dir`, so a
  generated workflow cannot write existing-comment metadata outside the
  workspace (#6789).
