<!-- section: Fixed -->
- `ripr diff --json` strips an absolute `--root` from `changed_seams[].file`
  before portable rendering, so those paths match `changed_files[].path`
  instead of leaking root components with the leading slash dropped (#7797).
