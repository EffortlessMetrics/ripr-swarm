<!-- section: Fixed -->
- A line whose text is byte-identical on the removed and added side of one
  replacement block (unchanged or moved inside a rewritten block) no longer
  seeds a changed-behavior probe. Suppression requires the text to occur
  exactly once on each side of the block; repeated or edited lines keep their
  probes (#8389).
