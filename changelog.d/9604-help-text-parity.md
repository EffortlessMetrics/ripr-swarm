<!-- section: Fixed -->
- `ripr help check` now documents `RIPR_CHECK_FINDINGS_BYTES` (findings-array byte budget for JSON output; 0 removes the cap, an unparseable value fails the run) and `ripr help doctor` documents `RIPR_CACHE_DIR` (relocates the cache base directory) (#9604).
- Usage lines now match the accepted options: `ripr pr-comments plan` names `--no-token`, `--write-permission`, and `--max-inline-comments`; `ripr rerun` names `--check-parity`; `ripr gate evaluate` points at the full Options list instead of implying only five inputs exist (#9604).
