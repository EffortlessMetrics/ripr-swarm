<!-- section: Internal / CI -->
- The `cli::interrupt` isolated-child test that raises an unregistered SIGINT no longer fails when the test process was started with SIGINT ignored or blocked (a detached background job, `nohup`, some CI and IDE runners): the child now restores the default action before raising, so it still dies by SIGINT and the harness still reports it (#12875).
