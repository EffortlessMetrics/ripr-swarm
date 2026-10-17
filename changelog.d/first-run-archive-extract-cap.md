<!-- section: Fixed -->
- The released-archive first-run route (`--install-archive`) now refuses to extract
  a `ripr` entry larger than 512 MiB and removes the partial file, so a malformed
  archive cannot fill the disk (#5311).
