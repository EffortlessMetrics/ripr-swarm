<!-- section: Changed -->
- TypeScript preview: a cold `ripr check` on a large TypeScript monorepo is about twice as fast (vscode 45 s to 21 s, angular 49 s to 22 s on 4 cores). Per-file facts for upcoming files are derived on parallel threads, and the enum-member guard masks and write-scans each module once instead of once per import. Output is unchanged (#13832).
