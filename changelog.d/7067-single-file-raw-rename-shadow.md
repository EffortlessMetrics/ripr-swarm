<!-- section: Fixed -->
- Owner-return pins and related-test reach now refuse a receiver type
  rebound by a single-file `use ... as <name>` rename in either spelling.
  A raw-identifier rename (`as r#Name`) escaped the single-file shadow
  check, so the renamed receiver matched production and the finding kept
  direct production reach. Both spellings now refuse the pin and the
  relation stays name-only (`weak_token_substring`), while a plain `use`
  still keeps its credit as a possible re-export (#7067).
