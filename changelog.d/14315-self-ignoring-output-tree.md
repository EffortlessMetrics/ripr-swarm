<!-- section: Changed -->
- ripr's default output folder `target/ripr` now ignores itself: the first command that writes there adds `target/ripr/.gitignore` (`*`), so Python and TypeScript projects no longer see `?? target/` in `git status` or get asked to edit their `.gitignore`. Your own `.gitignore` is never touched, and an existing `target/ripr/.gitignore` is kept (#14315).
