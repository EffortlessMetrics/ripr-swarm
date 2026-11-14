<!-- section: Fixed -->

- `ripr explain` and `ripr context`: a `file:line` miss no longer says the scope or file has no findings unless the run is known complete. When a fresh run's analysis was incomplete it says none were reported and that a finding there may be missing, stating the incompleteness once (#14406); on a replayed `--from` artifact, which does not record whether its run was complete, it says so instead (#14440).
