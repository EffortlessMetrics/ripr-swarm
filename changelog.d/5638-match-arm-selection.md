<!-- section: Fixed -->
- Match-arm findings now check which arm a related test's owner call selects
  (RIPR-SPEC-0229). An exact assertion credits the changed arm only when its
  direct call provably selects that arm; a test whose inputs all select other
  arms names the missing discriminator and the observed inputs. Unprovable
  patterns, earlier guarded arms, imported or aliased inputs from outside the
  workspace, inequality assertions and let-bound results stay unjudged. On the
  verdict corpus, false actionable verdicts fall from 66/106 to 64/106 with
  false exposed (6/97) and false silent (0/97) unchanged (#5432).
