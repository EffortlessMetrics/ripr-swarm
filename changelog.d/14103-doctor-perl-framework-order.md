<!-- section: Fixed -->
- `ripr doctor` names the same Perl test framework for the same `t/*.t`
  files on every machine. A suite mixing frameworks used to report whichever
  file the filesystem listed first, so `test_surfaces[].framework` could read
  `Test2::V0` on one checkout and `Test::More` on another (#14103).
