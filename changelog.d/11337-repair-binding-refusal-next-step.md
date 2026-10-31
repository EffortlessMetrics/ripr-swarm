<!-- section: Fixed -->
- `ripr agent receipt` and the repair after phase now say what to do when the
  after verdict binding no longer matches the attempt: output redirected into a
  repository file during the run, or a later edit, can cause it; a receipt
  retry removes the stray file or edit, while an after-phase caller keeps a
  copy of the repair edit, restores the pre-edit test content, runs the printed
  root-bound restart command, and re-applies the edit (#11337).
