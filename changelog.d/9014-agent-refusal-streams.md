<!-- section: Fixed -->
- `ripr agent card --json` and `ripr agent stub --json` now write their typed
  refusal document (`agent_card_refusal`, `rust_test_stub` `state: refused`) to
  stdout with the prose on stderr, matching `check`, `agent packet` and
  `agent verify-execute`; and an unknown `--seam-id` now exits 3 on `agent card`,
  `agent stub`, `agent packet` and `agent repair --phase before` (previously
  packet and repair exited 2). Other packet failures keep exit 2 (#9014).
