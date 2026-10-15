<!-- section: Fixed -->
- A default `ripr pilot` run that exhausts the automatic extension now
  reports the timeout against the 30000 ms default plus the 240000 ms
  extension, and the printed retry stays at an interactive bound that is
  never smaller than the time already spent, instead of suggesting
  `--timeout-ms 1080000` (#7427).
