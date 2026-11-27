<!-- section: Fixed -->
- `ripr check` no longer searches a whole function for a number (`17`) as if
  it could be a local copy of a parameter when it reads a changed comparison
  such as `a > 17`. A number is never a local, so the answer is now given
  without the search, as the per-function inventory already did. A long
  function with many changed comparisons checks faster (#11738).
