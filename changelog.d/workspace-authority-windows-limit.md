<!-- section: Changed -->
- The symlink-swap currentness tests now also run on Windows hosts that may
  create symlinks, and a Unix test pins that a file rewritten in place with
  the same size and a restored mtime is no longer reported current. On
  Windows that in-place rewrite stays a documented limit: std has no stable
  change time there (#6755).
