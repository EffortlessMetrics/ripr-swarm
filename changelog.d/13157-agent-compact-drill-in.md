<!-- section: Fixed -->
- `check --format agent` drill-in lines now carry exactly one `--format agent`,
  inserted before the selector, so following a printed `next=ripr explain ...`
  command keeps the compact agent surface and the full navigation scope
  instead of dropping back to the human format (#13157).
