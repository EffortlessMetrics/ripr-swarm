<!-- section: Fixed -->
- Effect-state whole-object observers that read a SCREAMING_CASE static or const root (`*LOG.lock()`, `TABLE[0]`) are no longer refused when the deleted callee may have written that state (#7070 item 1).
