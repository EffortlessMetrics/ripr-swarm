<!-- section: Security -->
- The dependency advisory check now states `unmaintained = "all"`, so
  unmaintained advisories on transitive crates are reported under
  cargo-deny 0.22 and later, whose default scope is direct workspace
  dependencies only (#15945).
