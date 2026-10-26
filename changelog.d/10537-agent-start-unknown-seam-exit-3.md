<!-- section: Fixed -->
- `ripr agent start` now maps an unknown `--seam-id` to the decision exit code `3`, matching `agent card`, `agent stub`, and `agent packet` for the identical cause; operational failures of the command stay exit `2`. Documented in `docs/EXIT_CODES.md` (#10537).
