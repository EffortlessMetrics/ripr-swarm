<!-- section: Fixed -->
- Repair: an attempt that follows a new-test proposal now says up front that
  it finishes only with a focused `--test-result FILE` and how to produce FILE.
  The start's next command, its `--json` continuation (additive
  `focused_result_requirement`) and `ripr agent status` name the flag. Running
  its after phase with no result at all (for example via `ripr continue`) is
  still refused with exit 3 and the same recovery text, but no longer spends
  one of the attempt's after-phase retries; a supplied failing or zero-count
  result still does. Attempts that follow an existing test are unchanged
  (#16164).
