<!-- section: Internal / CI -->
- Kani tool discovery now bounds the cargo-kani version probe through the
  shared process-capture deadline, reporting unavailable or timed-out tools
  without launching verification or allowing an empty-run override (#12579).
