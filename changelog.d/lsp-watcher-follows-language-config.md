<!-- section: Fixed -->
- The editor server's file watcher now follows `[languages.python]` and
  `[languages.typescript]` configuration. A file listed in
  `handwritten_files` refreshes when it is rewritten outside the editor even
  though its directory is excluded by default (for example `dist/`), and a
  file matched by `excluded_file_patterns` no longer triggers a refresh
  (#16080).
