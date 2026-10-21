<!-- section: Fixed -->
- `ripr baseline create` retains the source and output paths in its quoted
  overwrite retry, so the printed `--force` command can run against the
  intended baseline. Windows errors disclose the safe Git Bash route for
  special path characters and escape fragments (#9042).
