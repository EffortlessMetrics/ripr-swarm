<!-- section: Fixed -->
- Bare `ripr repair` no longer starts, or asks you to choose, a repair on a
  seam outside the lines your diff changed. A changed constant or private
  helper line that `ripr check` offers no repair start for now gets the same
  answer from `ripr repair`: the refusal names your changed lines, counts the
  repairable seams it did not offer, and says `ripr repair <seam-id>` still
  repairs one you name. The refusal for a seam in a state ripr does not start
  repairs for now says so in plain words instead of "its class is not a
  repair target under the current policy" (#11111).
