<!-- section: Changed -->
- Source-promotion preflight lists each parent's unfolded `changelog.d/`
  fragments and refuses a reviewed join tree that still holds one. The
  back-sync verifier refuses a K that still holds one, so a fragment can no
  longer miss the release notes silently (#6341).
