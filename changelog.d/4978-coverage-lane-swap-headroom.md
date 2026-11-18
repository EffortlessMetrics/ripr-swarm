<!-- section: Internal / CI -->
- `.github/workflows/coverage.yml` adds 8 GB of swap before the instrumented
  build, like the required test shards and Test Analytics. The former scheduled
  run on main (retired by #12973) was losing its runner about six minutes into compilation three days
  running (#4978).
