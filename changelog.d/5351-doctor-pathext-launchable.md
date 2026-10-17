<!-- section: Fixed -->
- `ripr doctor` on Windows no longer selects an interpreter-only PATHEXT entry
  (`.PS1`, `.VBS`, `.JS`) ahead of a working `.exe` exporter; only suffixes
  `CreateProcessW` can launch (`.COM`, `.EXE`, `.BAT`, `.CMD`) are tried (#5351).
