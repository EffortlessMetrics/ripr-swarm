<!-- section: Fixed -->
- `ripr check --format agent` no longer says a Python or TypeScript finding has
  "no guided repair route" when `ripr agent repair` can start its gap. It now
  reads the same repair-route decision as the human screen: the header adds
  `repairable=1`, the ending is `open=` instead of `stop=`, and `next=` is the
  `ripr agent repair --gap-id ...` command. `--format json` carries the same
  command in `canonical_next_action.stop.repair_route` (#13599).
