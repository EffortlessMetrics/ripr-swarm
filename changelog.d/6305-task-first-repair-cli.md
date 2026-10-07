<!-- section: Features -->
- Task-first repair commands (#6305): `ripr repair [<item>]` starts a repair
  attempt for one gap (selecting only when exactly one repair-eligible seam
  is visible, and printing a bounded selection or the honest setup/limitation
  action otherwise), `ripr continue [--attempt ID]` advances the current
  attempt through the accepted after path, and `ripr status [--attempt ID]
  [--json]` reports the typed attempt state with its canonical next action.
  The three commands delegate to the same RepairAttempt and selector services
  as the advanced `ripr agent repair` / `ripr agent status` spellings, which
  stay supported with unchanged behavior.
