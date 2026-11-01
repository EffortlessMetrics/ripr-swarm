<!-- section: Internal / CI -->
- The goldens gate no longer reports drift when a line is added above a JS/TS test that a fixture cites: the `<test file>:<line>` citation is masked in the human text and `check.json`, and the `line` of a test-file entry in `check.json` is masked. Source locations, blocker wording, test names and oracles stay compared (#12163).
