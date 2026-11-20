<!-- section: Fixed -->
- `repo-sarif` and `repo-sarif-compact` now report `run_status: "limited_seam_evidence"`, a pass-wide `producer_failure` limitation and a warning-level invocation notification when the seam-evidence pass fails, instead of presenting opaque stand-in seams as a complete result; healthy output is unchanged (#13340)
