<!-- section: Fixed -->
- `ripr assistant-loop health` repair commands kept only the first word of
  a quoted `--root` from the proof's agent command, so a checkout path with
  a space produced `--root '/work/my` with an unbalanced quote. The root is
  now copied as the whole shell word it was rendered as, so roots with
  spaces, shell metacharacters or control characters round-trip (#4000).
