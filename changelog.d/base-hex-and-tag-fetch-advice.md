<!-- section: Fixed -->
- An unresolved `--base` spelled like a commit id (7 to 64 hex digits) now names `git fetch origin <sha>` (with `--unshallow` in a shallow clone), and a release-tag-shaped name (`v9.9.9`) names `git fetch --tags origin`, instead of the branch-fetch advice that cannot restore either (#0).
