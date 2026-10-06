<!-- section: Performance -->
- `ripr agent repair --phase before`: the packet, start, and snapshot
  steps share one classified-seam inventory instead of reloading the
  same cached inventory three times. Warm before-phase wall time on
  the ripr checkout fell from 27.5s to 17.2s (medians of three runs);
  the latency trace attributes ~5s to the two eliminated duplicate
  cache loads, corroborated by interleaved old/new runs, with
  byte-identical workflow artifacts (#5301).
