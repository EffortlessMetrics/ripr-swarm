<!-- section: Fixed -->
- For a changed push into a vector or string the test passes in and counts,
  the `ripr repair` brief now suggests an exact check of that collection's
  full contents (`assert_eq!(buf, expected_buf)`), the check `ripr agent stub`
  writes, instead of "Suggested assertion: none". The suggested test name and
  the stub's test name now end in `full_contents` instead of
  `side_effect_observer` (#12711). The same suggestion and name also appear
  in the agent packet's recommended test name and its `cargo test` command,
  the evidence record, the agent brief, the repair card's assertion goal,
  `ripr agent card` and the editor hover, but only for a route that was
  already ready to repair and not already checked, suppressed or intentional.
  A contents-check stub written earlier under the old
  `<owner>_side_effect_observer` name is still reported as already written.
