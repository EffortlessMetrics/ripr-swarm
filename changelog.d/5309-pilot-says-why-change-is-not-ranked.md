<!-- section: Fixed -->
- `ripr pilot` no longer says no seam is on your change when the change's seams
  exist but pilot withholds them (#5309). It now says why none of them ranks:
  pilot withholds them because their static evidence is unknown or opaque, they
  are already gripped, or the seam limit left seams unanalyzed.
  `pilot-summary.json` adds `current_change.withheld_seams_in_change`.
