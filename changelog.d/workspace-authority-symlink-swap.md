<!-- section: Fixed -->
- A test file swapped for a symlink that points outside the workspace is no
  longer admitted as a repair target when the swap keeps the file's size and
  modification time. The file-currentness cache followed symlinks, so it
  reused its earlier "current" answer. It now also records whether each
  entry is a symlink and, on Unix, its device and inode (#5478).
