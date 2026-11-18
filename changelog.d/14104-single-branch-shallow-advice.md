<!-- section: Fixed -->
- In a single-branch clone (what `git clone --depth 1` creates), the missing-base message now names the `git remote set-branches origin "*"` step before the fetch. The fetch alone never brought the base branch there, so following the old advice led to a second failure (#14104).
