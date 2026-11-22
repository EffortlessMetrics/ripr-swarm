<!-- section: Internal / CI -->
- The Windows packaged qualification lane now installs the publish-stage
  archive the release ships (`cargo xtask publish-dry-run`), bound to the run's
  rehearsal receipt and the candidate commit, with `ripr-contract` taken from
  the archive packaged beside it. It previously packaged the checkout's own
  crate, which is never uploaded and could not resolve `ripr-contract` before
  publication (#14198, #2769).
