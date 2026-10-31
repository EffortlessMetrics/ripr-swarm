<!-- section: Internal / CI -->
- The relink-isolation unit test compares the isolated target directory by trailing components instead of an exact path, so it no longer fails on Windows where a driveless rooted path is rewritten against the process drive (#11602).
