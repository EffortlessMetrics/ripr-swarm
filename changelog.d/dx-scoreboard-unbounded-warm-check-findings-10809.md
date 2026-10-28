<!-- section: Fixed -->
- DX scoreboard: the measured warm `ripr check` now renders the full findings
  set (`RIPR_CHECK_FINDINGS_BYTES=0`), so the self-contradiction scan can read a
  large diff such as the 131-finding `ripr-swarm-2026-10` row instead of
  reporting the row `incomplete` from a byte-bounded prefix (#10809).
