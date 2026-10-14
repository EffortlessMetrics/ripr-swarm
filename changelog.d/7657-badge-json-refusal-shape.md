<!-- section: Docs -->
- `docs/EXIT_CODES.md` now describes `check --format badge-json` refusal as
  the badge family's degraded-state convention (`status: "warn"`,
  `message: "refused: <identity>"`, `analysis_outcome: null`, no
  `run_status` field), distinct from the `run_status: "refused"` envelope
  used by `repo-exposure-json` and `agent-seam-packets-json` (#7657).
