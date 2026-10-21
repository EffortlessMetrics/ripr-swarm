<!-- section: Changed -->
- Perl fact-packet analysis now resolves file, owner, change, test, oracle,
  relation, provenance and verify-command references through a keyed index
  built once per packet instead of linear scans per change and relation, so
  analysis time no longer grows quadratically near the 10,000-entry array cap
  (#8878). Output and fail-closed actionability are unchanged.
