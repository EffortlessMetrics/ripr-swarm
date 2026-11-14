<!-- section: Fixed -->
- A directory passed where a file belongs now reads `is a directory, not a
  file; pass the path of a file` (or, for an output path, `the destination is
  a directory, not a file`) instead of the host's `Is a directory (os error
  21)` / `Access is denied. (os error 5)`, in every command that reads
  through the shared bounded reader or writes through the atomic writer.
  `context --from` / `explain --from` read the check artifact through the
  bounded reader, so an artifact over 256 MiB (the cap every other CLI input
  already has) is now refused, and name `ripr check --write-artifact PATH`
  with the same root, scope and analysis options when it is missing; `cache export` with a missing `--root` names the flag; and
  `first-action` no longer calls a supplied optional input that it could not
  read a "missing required artifact" on top of the warning that names its
  cause (#14694).
