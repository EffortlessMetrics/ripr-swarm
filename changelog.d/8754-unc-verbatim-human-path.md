<!-- section: Fixed -->
- On Windows, human-facing path rendering now folds the `\\?\UNC\server\share`
  verbatim prefix to its plain `\\server\share` spelling everywhere
  `human_path` renders (check header, doctor, pilot, config diagnostics,
  artifact roots), matching the drive-letter cleanup. `\\?\Volume{guid}` and
  other verbatim forms with no plain spelling still render exactly as given,
  now as one policy owned by the shared renderer instead of a per-call-site
  rule (#8754).
