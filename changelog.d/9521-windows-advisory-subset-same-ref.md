<!-- section: Internal / CI -->
- The `windows-advisory-subset` storage tests now read the selector consumer from the same ref as the test script (a sparse checkout of the run's own workflow file), so a pull request branch behind `main` no longer pairs new quoting tests with an old consumer and fails; the product build still checks out the exact PR head (#9521).
