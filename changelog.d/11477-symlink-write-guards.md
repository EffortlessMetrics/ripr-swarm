<!-- section: Fixed -->
- `[perl].cache_dir` and `ripr agent stub --write` no longer follow a committed directory link out of the workspace: the Perl facts cache and the stub's new `tests/` file are refused, with a next step, when their existing path (or a `..` component) resolves outside the analyzed root, before anything is created (#11477 items 2 and 4).
