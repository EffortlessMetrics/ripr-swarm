<!-- section: Fixed -->
- Probe-shape ranking on one long changed line is now linear in the line length and the number of distinct shape-text lengths, not `shapes × line`. Selection and the #6731 unchanged-field tie-break stay the same; only the `str::contains` scan is replaced (#8647).
