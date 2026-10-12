<!-- section: Fixed -->
- Callee-only (`seam_callee_call`) relations no longer report `reach: yes`:
  the reach stage now reads `weak`/`low`, agreeing with its own summary
  ("the changed owner is not invoked by them") instead of contradicting it.
  Such findings no longer suppress the all-no-path honesty note, and their
  classification hint reads "no test is seen calling this change" rather
  than claiming a reaching test. Class stays `weakly_exposed` via the
  `wrapper_error_binding_unresolved` cap (#3700); owner-anchored and
  proximity-only reach behavior is unchanged (#7003).
