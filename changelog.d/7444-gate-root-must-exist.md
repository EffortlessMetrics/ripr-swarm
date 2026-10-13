<!-- section: Fixed -->
- `ripr gate evaluate --root` that is missing or not a directory now
  fails closed (exit 2) and names the root, instead of binding a
  decision to a path that is not there (#7444).
