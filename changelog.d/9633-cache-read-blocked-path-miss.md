<!-- section: Fixed -->
- A cache read through a path component that is a file is a cache miss instead
  of also being counted as `corrupt_ignored`; the failed write remains the one
  reported store failure (#9633).
