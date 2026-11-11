<!-- section: Fixed -->
- `ripr check` no longer slows down cubically on a function with many changed
  comparisons: the parameter-rebinding rule reads each owner body in one pass
  per comparison instead of re-reading the whole body for every mention of the
  compared name. A diff that changes many functions of one file parses that
  file once for the macro-defined-name gate instead of once per changed
  function. Findings are unchanged (#11738).
