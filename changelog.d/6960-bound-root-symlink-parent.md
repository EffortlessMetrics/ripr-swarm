<!-- section: Fixed -->
- Generated commands no longer bind a different checkout when `--root` goes
  through a symlink and then `..` (for example `link/../repo`). On Unix the
  bound root and its redirect targets keep a `..` that follows an existing
  symlink, so the printed path resolves to the directory that was analyzed,
  as the operating system resolves it. Other roots render as before (#6960).
