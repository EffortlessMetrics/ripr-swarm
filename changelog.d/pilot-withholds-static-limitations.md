<!-- section: Changed -->
- Pilot: seams whose static evidence is unknown or opaque (`opaque` and the
  `*_unknown` grip classes) are no longer ranked as gaps. Pilot withholds
  them, counts them in the new `withheld_static_limitations_total` field of
  `pilot-summary.json` (schema `0.3`), and names the count in the terminal
  and `pilot-summary.md`. A run that withholds every seam says the empty
  ranking is not a clean result, and `ripr agent status` stops with the new
  `pilot_withheld_static_limitations_no_repair_target` warning instead of
  sending you back to pilot. When a seam limit cut the run, both name
  raising the limit, since seams past the cut may hold gaps. The seams stay
  in `repo-exposure.json`
  ([#5497](https://github.com/EffortlessMetrics/ripr-swarm/issues/5497)).
