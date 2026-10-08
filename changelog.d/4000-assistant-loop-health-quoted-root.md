<!-- section: Fixed -->
- `ripr assistant-loop health` repair commands kept only the first word of
  a quoted `--root` from the proof's agent command, so a checkout path with
  a space produced `--root '/work/my` with an unbalanced quote. The root is
  now decoded with shell quoting and re-quoted whole; a root that would run
  a command substitution or expand a variable is refused rather than copied
  from the proof into the repair command (#4000).
