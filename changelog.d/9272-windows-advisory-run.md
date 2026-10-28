<!-- section: Internal / CI -->
- The single-job Windows advisory lane runs its three test steps under the new
  `cargo xtask windows-advisory-run`: output streams to the job log as it
  arrives, the runner ends the whole process tree at its own deadline, names
  tests still running after 60 seconds when it does, and terminates
  descendants that keep the output pipe open after cargo exits. Test outcomes
  stay advisory and a missing status is still reported as missing evidence
  (#9272, #9660).
