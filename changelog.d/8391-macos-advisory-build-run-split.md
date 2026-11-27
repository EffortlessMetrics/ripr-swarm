<!-- section: Changed -->
- macOS Advisory builds the `ripr` library test executable in one job and runs
  it in another, because the build (about 30 minutes) and the tests (about 14)
  do not fit one 40-minute job together (#8391).
