<!-- section: Fixed -->
- `ripr check --format agent` keeps its suggested `ripr explain` drill-ins in
  compact agent format for reports without an explicit suppression policy,
  including reach-limited open findings. Finding selectors, quoting and the
  original analysis scope are preserved. Policy-bearing reports keep their
  existing human route because compact explain does not accept artifact replay
  with a suppression policy; context and other commands are unchanged (#13157).
