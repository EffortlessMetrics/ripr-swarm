<!-- section: Changed -->
- `server-archive-qualification` takes `candidate_ref` in the same
  `refs/tags/ripr-release-<version>-<candidate_sha>` grammar as the Windows
  packaged lane, and `release-server-binaries` forwards its optional
  `candidate_ref` instead of hard-coding an empty tag (#2769).
