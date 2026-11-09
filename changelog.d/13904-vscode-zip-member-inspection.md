<!-- section: Security -->
- On Windows, VS Code managed-server downloads now read the zip central directory before `Expand-Archive` runs. They refuse absolute, drive-qualified, `..`, `:` stream, trailing dot/space and reserved device-name member paths, symlink or reparse-point entries, malformed, ZIP64 or comment-smuggled archives, and archives over 1000 members (#13904).
