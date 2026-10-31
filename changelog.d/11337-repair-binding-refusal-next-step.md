<!-- section: Fixed -->
- `ripr agent receipt` and the repair after phase now say what to do when the
  after verdict binding no longer matches the attempt: output redirected into a
  repository file during the run, or a later edit, can cause it; a receipt retry
  removes the stray file or edit, while an after-phase caller sets the repair
  edit aside, starts a new attempt with the printed root-bound restart command,
  and restores the edit (#11337).
