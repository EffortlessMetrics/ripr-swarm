<!-- section: Fixed -->
- For a changed push into a vector or string the test passes in and counts,
  the `ripr repair` brief now suggests an exact check of that collection's
  full contents (`assert_eq!(buf, expected_buf)`), the check `ripr agent stub`
  writes, instead of "Suggested assertion: none". The suggested test name and
  the stub's test name now end in `full_contents` instead of
  `side_effect_observer` (#12711). The suggestion text also appears in
  `ripr agent card`, the card's JSON assertion goal and the targeted test
  outline's assertion guidance (the editor hover's suggested test shape). The
  new test name (and the `cargo test` command that uses it) also appears in
  the agent packet, the agent brief, the workflow output and the evidence
  record. All of this
  applies only to a route that was already ready to repair, not one already
  checked, suppressed or intentional. A contents-check stub written earlier
  under the old `<owner>_side_effect_observer` name (including a numbered
  `_2` name) is still reported as already written.
