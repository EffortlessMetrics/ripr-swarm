<!-- section: Internal / CI -->
- `.github/workflows/coverage.yml` adds 8 GB of swap before the instrumented
  build, like the required test shards and Test Analytics. The scheduled run on
  main was losing its runner about six minutes into compilation three days
  running (#4978).
