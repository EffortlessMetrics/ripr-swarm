<!-- section: Fixed -->
- Per-command help (`ripr help <command>` and `ripr <command> --help`) now renders an `Examples:` section built from the command catalog's `example` field — the same string `ripr help --json` emits — so every command shows a runnable invocation; `check`'s richer hand-written examples are unchanged (#9864).
