<!-- section: Fixed -->

- The repair spellings `ripr repair`, `ripr continue`, `ripr status` and their `agent` counterparts now refuse at once when no workspace, package or Git repository anchors the current directory, instead of reading or answering about an unrelated tree (`repair`/`continue` exit 3 with code `unanchored_root`; `agent` spellings keep their root-error exit). An explicit `--root` skips the gate (#12417).
