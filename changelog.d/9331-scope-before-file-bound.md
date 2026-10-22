<!-- section: Fixed -->
- `ripr check --path`/`--exclude` now apply while the diff is parsed, so a diff whose bulk is out of scope (a committed vendored tree, for example) no longer trips the changed-file bound before the scope can narrow it. The out-of-scope limitation still reports the exact number of files left out (#9331).
