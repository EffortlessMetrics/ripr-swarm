<!-- section: Fixed -->
- Related tests that call a wrapper seam's converted callee now report
  `seam_callee_call` instead of an owner-name or same-file relation, so a
  captured call is not hidden behind name or file proximity (#5335).
