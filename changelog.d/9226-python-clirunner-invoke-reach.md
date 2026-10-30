<!-- section: Fixed -->
- A Python edit in a click command, or in a helper the command calls, is no longer `no_static_path` when a test passes the command to `CliRunner.invoke(command, ...)` through a recognised runner: it gets a weak, reach-only `cli_runner_invoke` relation and never `exposed` from it (#9226).
