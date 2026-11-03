<!-- section: Internal / CI -->
- The isolated policy-facade test no longer gives its child a 15-minute deadline that is shorter than the 30-minute Cargo build budget `check-file-policy` itself allows; Windows runs finished in about 14 of 15 minutes and timed out on a slow runner (#12510).
