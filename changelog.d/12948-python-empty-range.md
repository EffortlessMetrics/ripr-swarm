<!-- section: Fixed -->
- Python range-header edits no longer read `exposed` from an exact output pin
  whose input leaves the returned sum unchanged. Unreadable ranges and other
  loop bodies remain unknown. Only the asserted call supplies range credit;
  discarded calls do not open exposure or repair credit (#12948).
