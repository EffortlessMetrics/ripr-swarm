<!-- section: Internal / CI -->
- Kani tool discovery now bounds the cargo-kani version probe through the
  shared process-capture deadline, reporting unavailable or timed-out tools
  without launching verification or allowing an empty-run override. Typed
  capture failures distinguish missing executables, denied launches, missing
  Kani, capture and process-control failures without publishing raw tool text.
  Linux front-door controls have an independent watchdog that owns and stops
  nested probe descendants. A maintained narrow test feature executes the
  actual process-owner source and lane API when the full analyzer cannot be
  built locally; full CLI and native proof qualification remain separate
  (#12579).
