# RIPR Gate Decision

Decision: config_error
Mode: baseline-check
Evaluated: 0
Blocking: 0
Acknowledged: 0
Advisory: 0

## Config Errors

- baseline-check mode requires an explicit --baseline artifact (RIPR_GATE_BASELINE in the generated workflow). To start without blocking, use --mode visible-only (RIPR_GATE_MODE=visible-only). To adopt baseline-check, create a baseline from a gate decision with `ripr baseline create --from target/ripr/reports/gate-decision.json`, commit it, and pass it with --baseline (see docs/BASELINE_LEDGER_WORKFLOW.md)

## Limits

Optional policy over static RIPR evidence; advisory by default; runtime mutation calibration is used only when supplied.
