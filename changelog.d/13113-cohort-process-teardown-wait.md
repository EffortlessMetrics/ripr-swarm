<!-- section: Internal / CI -->
- The agent-cohort harness's process-control test polls the owned
  descendant's `/proc` state for up to five seconds before asserting, so
  asynchronous SIGKILL delivery and reparenting under runner load no longer
  flip the combined-tree proof (#13113, #13443).
