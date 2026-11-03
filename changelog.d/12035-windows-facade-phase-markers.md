<!-- section: Internal / CI -->
- The Windows advisory shards now cap tool builds at 300 s, below nextest's 360 s
  kill, and the policy-facade test announces each check on stderr, so a stalled
  child names the check it was in instead of an anonymous
  TIMEOUT row (#12035).
