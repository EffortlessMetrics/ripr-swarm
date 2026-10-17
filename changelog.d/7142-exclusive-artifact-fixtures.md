<!-- section: Internal / CI -->
- Artifact identity test fixtures allocate private directories exclusively and
  preserve occupied roots while handling name collisions (#7142).
- Give Test Analytics a 40-minute timeout, raised from 30 to cover its
  all-features workload, while retaining its test selection, doctests and
  failure handling (#7142).
