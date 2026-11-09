<!-- section: Fixed -->
- `repo-exposure-json` now reports `run_status: "limited_seam_evidence"` with one pass-wide `producer_failure` limitation when the seam-evidence pass fails, instead of presenting opaque stand-in seams as a complete inventory; healthy output is unchanged, and the Markdown, SARIF and agent-packet renderers are not covered yet (#13340)
