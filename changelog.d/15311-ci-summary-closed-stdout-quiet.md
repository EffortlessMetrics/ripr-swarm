<!-- section: Fixed -->
- `ripr reports ci-summary | head` (or any reader that closes stdout early)
  now exits `2` quietly like every other command, instead of printing a
  misleading "could not write the summary ... check you can write to the
  summary path" error (#15311).
