<!-- section: Internal / CI -->
- The `ci` nextest profile now has a `slow-timeout`: a test running past two minutes prints a SLOW row, and one running past twenty minutes fails as a named TIMEOUT row instead of silently consuming the job until its cap. Passing tests are unaffected (#12092).
