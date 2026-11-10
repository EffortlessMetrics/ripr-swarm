<!-- section: Security -->
- The VS Code extension's managed Windows server download now inspects the zip central directory (unsafe names, symlink and reparse-point members, member-count overflow) before `Expand-Archive` writes anything, matching the pre-extraction contract the tar path already keeps (#13904).
