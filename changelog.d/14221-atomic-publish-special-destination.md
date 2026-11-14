<!-- section: Fixed -->
- `feedback export --out`, `check --write-artifact` and the other outputs that
  publish through the atomic writer no longer replace a device, FIFO or socket
  named as the destination (directly or through a symlink). They used to rename
  a regular file over the node and exit 0, which as root could destroy a node
  such as `/dev/null`; they now exit 2, say why, and leave the node untouched
  (#14221).
