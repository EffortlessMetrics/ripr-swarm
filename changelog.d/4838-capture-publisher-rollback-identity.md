<!-- section: Internal / CI -->
- `cargo xtask merge-queue capture` distinguishes App and installation IDs,
  retains required-check publisher bindings and repository rollback settings,
  and rejects stale repository, workflow or installation evidence (#4838).
- Capture refuses unsuccessful collectors with partial successful output,
  requires readable details for every preserved ruleset before reporting a
  complete rollback snapshot, and keeps persisted receipt identities stable
  without suppressing real ruleset drift (#4838).
