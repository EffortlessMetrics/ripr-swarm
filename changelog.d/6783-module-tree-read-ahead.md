<!-- section: Changed -->
- `ripr check` reads ahead in the module-tree walk that places each changed
  file, scanning the queued files on all cores instead of one at a time. A
  warm run on bevy's pinned corpus diff drops from about 2.1 s to 1.8 s with
  #6629, with identical output (#5320).
