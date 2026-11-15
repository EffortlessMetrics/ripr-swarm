<!-- section: Internal / CI -->
- Admission selects the latest stated source-review review per login, so a
  neutral, empty or prose follow-up no longer erases that login's marker, and a
  mis-formatted positive marker (`REVIEW_READY <sha>`) is neutral instead of a
  blocking refusal, so one login's tail formatting cannot veto another
  carrier. A comment clears that login's negative only when it is trusted and
  posted at the admitted head (#14728).
