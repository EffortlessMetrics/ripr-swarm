<!-- section: Changed -->
- `--format json-terse` now codes repeated `ripr.<stage>.summary` strings
  through `evidence_legend`, sharing the code of the identical `evidence`
  entry they usually repeat. On a four-finding `dtolnay/semver` diff this
  cuts the terse document from 18,288 to 16,972 bytes. Stage `state` and
  `confidence` stay verbatim and `--format json` is unchanged (#13673,
  #14308).
