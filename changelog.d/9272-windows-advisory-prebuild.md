<!-- section: Internal / CI -->
- `.github/workflows/windows-advisory.yml` builds every workspace test binary in its own advisory step before the two nightly stability samples, so run 1 no longer reaches its deadline during the cold compile and both runs observe the same tests. Run 2's deadline moves from 70 to 90 minutes and the job limit to 360 (#9272).
