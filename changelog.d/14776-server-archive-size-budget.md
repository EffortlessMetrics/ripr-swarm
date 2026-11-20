<!-- section: Internal / CI -->
- Tighten the server-archive payload budget in `xtask` from 96 MiB to
  64 MiB (67,108,864 bytes): the measured archive is about 24 MiB
  (24,092,759-byte gzip-9 executable proxy plus small licence/readme files),
  so the new ceiling is roughly 2.6x current size and remains a
  catastrophe tripwire checked via `check_archive_size_budget`, while the
  128 MiB executable and 160 MiB total-payload ceilings are unchanged (#14776).
