<!-- section: Fixed -->
- `check-evidence-promotion-honesty` now also requires a `report_subject` (and regenerates the report) for cases that pin a repo seam only through `expected_repo_seam_related_test`, so those cases can no longer read a stale capture with the gate green (#12188).
