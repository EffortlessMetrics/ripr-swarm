<!-- section: Fixed -->
- The "Speed and memory" section of `docs/PUBLIC_PROOF.md` no longer claims it
  was measured on "the pinned corpus repositories": the four measured
  repositories are pinned in `benchmarks/dx_scoreboard/scoreboards.toml`, a
  different population from the 34-repository corpus manifest (0 of the 4
  appear there at the same revision). The prose now names the real population
  and counts the manifest overlap from the committed receipts (#8719).
