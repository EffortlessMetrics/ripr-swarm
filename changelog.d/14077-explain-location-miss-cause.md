<!-- section: Changed -->
- `ripr explain <file:line>` and `ripr context --at <file:line>` now say why
  a location has no finding: they name the finding lines when the file has a
  finding at another line, name the analyzed root when the text is not a
  file path from it, and otherwise give the run's changed-file, changed-line and candidate
  counts, or say the file may not have been analyzed when the run was
  incomplete. One sentence used to answer all of these (#14077).
