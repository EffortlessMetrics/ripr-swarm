<!-- section: Fixed -->
- `ripr pilot` and the repo-wide `repo-exposure`/`repo-sarif` formats skip an
  unreadable unchanged Rust directory or file and report it as a limitation
  instead of aborting; pilot ranks no gap and offers no repair while a path was
  skipped, and `ripr status` says to restore read access (#13624).
