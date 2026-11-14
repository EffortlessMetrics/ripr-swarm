<!-- section: Internal / CI -->
- An adapter-substrate qualification now refuses a safety receipt reference whose `no_execution`, `no_network`, `root_containment` or `process_tree_bounded` flag is false (`AuthorityViolation`, naming each unestablished flag); a stale reference keeps `StaleAuthorityReference` (#13034).
