<!-- section: Fixed -->
- For a changed push into a vector or string the test passes in and counts,
  the `ripr repair` brief now suggests an exact check of that collection's
  full contents (`assert_eq!(buf, expected_buf)`), the check `ripr agent stub`
  writes, instead of "Suggested assertion: none". The suggested test name and
  the stub's test name now end in `full_contents` instead of
  `side_effect_observer` (#12711).
