<!-- section: Fixed -->
- `ripr check` no longer offers `ripr agent stub --seam-id ...` choices that the
  command would refuse as `not_a_gap`: when a line holds several tied seams,
  only the ones the stub route accepts are listed, and a refusal line replaces
  the commands when none qualifies. `ripr agent stub --at ... --kind ...` on a
  seam the inventory grades `strongly_gripped` no longer says "a stub would add
  nothing" and then prints one; the warning now calls the stub an optional
  scaffold, not evidence of a missing discriminator (#10192).
