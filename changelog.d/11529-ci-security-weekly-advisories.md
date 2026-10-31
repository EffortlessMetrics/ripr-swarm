<!-- section: Changed -->
- CI security posture: `security.yml` now runs the `cargo-deny` advisory/licence/ban/source check weekly on `main` (and on demand) so new RUSTSEC entries surface without a merge; Dependabot cargo and npm updates wait 7 days after release; the `droid*` workflows default to read-only token permissions (#11529).
