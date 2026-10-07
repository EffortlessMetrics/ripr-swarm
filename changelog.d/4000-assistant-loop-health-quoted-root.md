<!-- section: Fixed -->
- `ripr assistant-loop health` repair commands kept only the first word of
  a quoted `--root` from the proof's agent command, so a checkout path with
  a space produced `--root '/work/my` with an unbalanced quote. The root is
  now read with shell quoting and re-quoted whole (#4000).
