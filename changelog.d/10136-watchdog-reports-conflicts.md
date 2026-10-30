<!-- section: Fixed -->
- The 30-minute PR staleness watchdog now tells merge-ready PRs that conflict
  with their base, with one comment and a failing `merge-ready conflict` status,
  instead of only listing them in a step summary. This reaches PRs labeled
  before the label-time notice existed and PRs that start conflicting after main
  moves, and it clears its own failure once the conflict is gone (#10136, #9842).
