<!-- section: Fixed -->
- TypeScript tests registered in a loop over data the analyzer cannot enumerate (a file listing, a parameter, a call result) are now indexed for reach under the loop head instead of disclosed as unextracted. They earn no assertion credit, so a finding resting only on them reads `static_unknown` (#10501).
