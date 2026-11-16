<!-- section: Fixed -->
- `ripr check`: a line that only respells a name the same file's diff renames
  at its declaration (a `static`, `const`, `fn` or `type` whose item is
  otherwise unchanged, with no old-name use left and no new-name use in the
  base) is no longer a changed behavior, so "Start here" no longer picks a
  pure rename over the diff's real change. (#14760)
