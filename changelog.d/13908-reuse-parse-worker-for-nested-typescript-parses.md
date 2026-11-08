<!-- section: Changed -->
- TypeScript preview: a cold `ripr check` on a large TypeScript monorepo is about a third faster again (vscode 21 s to 15 s, angular 22 s to 14 s on 4 cores). A parse requested while a file's facts are being derived, such as a literal mask, now runs on the current parse worker instead of starting a new 256 MiB thread. Output is unchanged (#13908).
