<!-- section: Internal / CI -->
- The disk-equality cache test moves the file mtime explicitly after a same-length rewrite, so it no longer depends on filesystem timestamp resolution (#9272).
