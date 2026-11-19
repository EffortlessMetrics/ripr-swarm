<!-- section: Changed -->
- The `assert!(matches!(..))` admission check scans the workspace for
  `matches` binding sites once per index instead of once per analysis
  context. It skips files whose stored macro candidates rule the name out,
  and no longer holds the per-file verdict lock while scanning. A warm
  `ripr check --base HEAD~1` over ripr-swarm that needs the check drops from
  about 15.5 s to about 14.3 s, with byte-identical JSON (#13754).
