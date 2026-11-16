<!-- section: Fixed -->
- Provisioning controls: the owned-process fixtures now record the group TERM
  from a supervisor blocked in `wait(2)` instead of the supervised Node event
  loop, so a loaded runner can no longer SIGKILL the fixture before it writes
  its acknowledgement and fail the cancellation controls with "parent did not
  observe the owned group TERM" (#14927).
