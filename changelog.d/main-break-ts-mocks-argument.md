<!-- section: Fixed -->
- Main compiled again after #10456 and #10488 crossed: the two registrations
  collected under an unevaluable `if` guard still passed the file-wide `mocks`
  list that #10488 moved off `collect_tests_from_statements`. No behaviour
  change.
