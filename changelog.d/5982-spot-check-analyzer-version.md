<!-- section: Internal / CI -->
- The mutation spot-check receipt now records `analyzer_version` (the `ripr --version` line, version plus commit) of the build that produced its static classifications, and the run fails if the binary cannot report one (#5982).
