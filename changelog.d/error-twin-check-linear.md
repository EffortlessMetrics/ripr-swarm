<!-- section: Fixed -->
- Repository inventory stays fast on generated files with deep method chains
  that share a start byte. Before, the duplicate `error_variant` check
  compared those shapes pairwise. 100k shapes at one start took about a
  minute, and two 20k-deep chains side by side ran for more than ten minutes.
  Both now take milliseconds. Which seams are kept is unchanged (#6954).
