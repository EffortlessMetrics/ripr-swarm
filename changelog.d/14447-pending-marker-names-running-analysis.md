<!-- section: Fixed -->
- Editors: before the first analysis finishes, the "no findings yet" marker
  now says analysis is in progress instead of asking you to save or run
  `ripr.refresh` to start it. On a large workspace the old text stayed up for
  the whole first run. The save/refresh advice returns only when no run is
  under way. (#14447)
- Editors: the "no findings yet" marker no longer offers an "Inspect finding:
  copy context packet" action, since it is not a finding. (#14447)
