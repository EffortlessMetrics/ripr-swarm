<!-- section: Internal / CI -->

- The scheduled `cargo-deny` run on main is now daily and opens one `deny-red` tracking issue when it fails (closing it on the next green run), so a new RUSTSEC advisory is a single findable record before it turns every PR red (#11529).
