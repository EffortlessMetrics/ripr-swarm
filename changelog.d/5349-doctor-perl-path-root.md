<!-- section: Fixed -->
- `ripr doctor` no longer executes a Perl exporter (`perl-ripr-facts` /
  `perllsp`) found on PATH when its canonical path lies inside the analyzed
  checkout, unless `RIPR_ALLOW_REPO_PERL_EXECUTABLE=1` is set. The hit is
  reported as `refused` (not run) with the canonical resolved path that was
  judged as the reason and the opt-in / move-the-exporter-outside-the-repo
  next step, on the default human screen, in `--verbose`, and in `--json`
  output (`exporter.state: "refused"`), and a refusal takes precedence over
  an incompatible hit. Symlinks and relative PATH entries pointing into the
  root are refused by canonicalization; a candidate that cannot be
  canonicalized is judged by its ancestor-resolved location and fails
  closed when that location is inside the root, while exporters outside
  the root reached through a relative PATH entry still probe as before.
  Approved exporters are executed through the canonical path that was
  judged, so swapping the PATH spelling cannot redirect execution
  (replacing the canonical target file itself remains a residual race).
  `[perl].executable` gating is unchanged (#5349).
