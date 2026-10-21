<!-- section: Fixed -->
- `ripr doctor` no longer executes a Perl exporter (`perl-ripr-facts` /
  `perllsp`) found on PATH when its canonical path lies inside the analyzed
  checkout, unless `RIPR_ALLOW_REPO_PERL_EXECUTABLE=1` is set. The hit is
  reported as `refused` (not run) with the resolved in-checkout path as the
  reason and the opt-in / move-the-exporter-outside-the-repo next step, in
  both human and `--json` output (`exporter.state: "refused"`). Symlinks and
  relative PATH entries pointing into the root are refused by
  canonicalization; exporters outside the root reached through a relative
  PATH entry still probe as before, and `[perl].executable` gating is
  unchanged (#5349).
