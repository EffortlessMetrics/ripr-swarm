<!-- section: Internal / CI -->
- The termination qualification harness stops Git discovery at the case root's parent and removes `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE` from each child, and a new execution control plants a decoy repository above the root and requires the child not to find it (#9480).
