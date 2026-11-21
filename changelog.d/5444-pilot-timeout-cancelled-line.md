<!-- section: Fixed -->
- A timed-out `ripr pilot` run now always ends its stderr progress with one
  `ripr progress: cancelled [repo]` line. The line used to depend on whether
  the analysis worker stopped within its five-second grace (#5444).
