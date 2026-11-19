<!-- section: Internal / CI -->
- `.github/workflows/issue-staleness-watchdog.yml` sweeps open issues weekly,
  flags those whose cited repository paths changed on `main` after filing,
  applies `status/needs-reverify` and maintains one marker comment per issue
  (#12634).
