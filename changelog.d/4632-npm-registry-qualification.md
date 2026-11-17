<!-- section: Added -->
- The release package-family rehearsal now runs the npm install-to-use
  journey on all five targets through an isolated loopback registry, then
  gates each target's receipt against the candidate commit, tree, launcher
  tarball and qualified payload digest. A missing, failed or mismatched row
  fails the aggregate (#4632).
