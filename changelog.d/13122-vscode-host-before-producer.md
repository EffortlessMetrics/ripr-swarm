<!-- section: Internal / CI -->
- The VS Code test runner acquires its editor host before preparing the genuine
  actionable-queue fixture, so blocked downloads fail before invoking that
  producer. Contributor instructions clarify the equivalent full-suite commands
  and host prerequisites (#13122).
