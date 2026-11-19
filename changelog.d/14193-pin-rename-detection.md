<!-- section: Fixed -->
- `ripr check` no longer changes its answer with the `diff.renames` git
  setting. With `diff.renames=false` a renamed and edited file was read as a
  whole new file, so every line of it was reported, and with `copies` a new
  file could be read as only its difference from another file. Rename
  detection is now pinned to git's default for every analysis diff (#14193).
