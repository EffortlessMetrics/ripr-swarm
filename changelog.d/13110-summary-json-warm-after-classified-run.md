<!-- section: Changed -->
- A classified repo-wide run now also stores the compact cache family, so a
  following `repo-exposure-summary-json` run on the same workspace loads warm
  instead of re-deriving every seam. A capped run stores nothing in the
  compact family, which stays a complete inventory. Stdout documents are
  unchanged; a `ripr cache export` of one current key now carries three
  recognized entries (classified, compact and file facts) where it carried
  two (#13110).
