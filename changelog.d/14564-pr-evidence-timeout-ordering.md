<!-- section: Internal / CI -->
- Preserve the workflow's 3300-second inner analysis deadline instead of
  overriding it with the 3600-second PR-evidence wrapper timeout, so typed
  `analysis_timeout` refusals arrive before wrapper termination (#14564).
