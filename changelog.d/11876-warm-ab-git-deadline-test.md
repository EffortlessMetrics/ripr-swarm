<!-- section: Internal / CI -->
- `warm-ab` has a test that a stalled git remote times out at its deadline and the next git call still runs, covering the per-pin clone deadline added with `--external-pins` (#11876).
