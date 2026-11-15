<!-- section: Internal / CI -->
- Admission selects the latest stated source-review review per login, so
  recognised chatter and an empty body no longer erase that login's marker, and
  a mis-formatted positive marker (`REVIEW_READY <sha>`) with nothing after it
  is neutral instead of a blocking refusal, so one login's tail formatting
  cannot veto another carrier. Any other prose still supersedes the marker and
  keeps main's reading, so a plain-English retraction or objection still
  refuses rather than falling through to the description carrier. A comment
  clears that login's negative only when it is trusted and posted at the
  admitted head (#14728).
