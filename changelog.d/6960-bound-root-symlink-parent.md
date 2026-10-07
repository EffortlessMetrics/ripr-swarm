<!-- section: Fixed -->
- Generated commands no longer bind a different checkout when `--root` goes
  through a symlink and then `..` (for example `link/../repo`). On Unix the
  bound root and its redirect targets keep a `..` that follows an existing
  symlink, so the printed path resolves to the directory that was analyzed,
  as the operating system resolves it. A gap-ledger refresh keeps a declared
  repo-exposure source that stays under such a root instead of falling back
  to the default path. A declared source that escapes the root, names the
  root itself, or uses `..` after a symlink inside the checkout still falls
  back to the default. Roots without a symlink followed by `..` render as
  before (#6960, #7017).
