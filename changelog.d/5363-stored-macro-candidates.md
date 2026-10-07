<!-- section: Changed -->
- A warm `ripr check` on a large workspace is faster again: parser-backed
  file facts now store the trusted macro names a file could rebind, so the
  scans that look for a workspace rebinding of `assert_eq!` and the other
  trusted macros skip parsing files that cannot report the requested name.
  The file-fact cache schema moves to `1.33`, so the first run after upgrade
  rebuilds that cache. On ripr-swarm (pin 7c64b9c) the warm check drops from
  about 4.4 s to about 3.2 s, and its JSON is byte-identical to main's
  (#5363).
