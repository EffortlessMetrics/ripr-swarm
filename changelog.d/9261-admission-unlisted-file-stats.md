<!-- section: Fixed -->
- Candidate admission no longer counts lockfile lines against the size envelope when GitHub omits per-file line counts for files past its diff budget; the omitted lines are the PR total minus the listed ones, with hand-written files in that group bounded by their own line counts (#9261).
