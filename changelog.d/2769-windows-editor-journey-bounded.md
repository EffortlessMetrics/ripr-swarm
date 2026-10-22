<!-- section: Internal / CI -->
- The Windows packaged editor journeys (`windows-packaged-qualification.yml`) are now bounded at 30 minutes per step, and the VS Code host logs are retained in the receipts even when a journey hangs or fails, so a stuck editor lane fails with evidence instead of running to the 120-minute job limit with no output (#2769).
- The real multi-root editor acceptance test reads the copied text from the test clipboard capture file instead of reading the Windows runner's clipboard straight back, which returned the previous copy in rehearsal run 39705039321 (#2769).
