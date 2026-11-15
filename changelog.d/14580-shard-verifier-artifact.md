<!-- section: Internal / CI -->
- Bind the shard verifier executable to an exact same-run artifact and producer
  checkout, validate its digest and unique regular members, and launch from fresh
  runner-temporary storage instead of searching candidate files (#14580).
